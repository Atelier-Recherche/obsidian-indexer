import {
	App,
	MarkdownView,
	Modal,
	Notice,
	Platform,
	Plugin,
	PluginSettingTab,
	Setting,
	TFile,
	normalizePath,
} from "obsidian";
import initSqlJs, { Database } from "sql.js";
import sqlWasmBinary from "./sqlWasm";
import { spawn } from "child_process";
import { mkdirSync, writeFileSync } from "fs";
import path from "path";

interface VaultIndexSettings {
	dbRelativePath: string;
	trayExecutablePath: string;
}

const DEFAULT_SETTINGS: VaultIndexSettings = {
	dbRelativePath: ".obsidian-index/index.sqlite",
	trayExecutablePath: "",
};

/** Cache WASM loader (expensive). Le binaire est embarqué dans main.js, pas de fichier séparé. */
function createSqlLoader() {
	let cached: Awaited<ReturnType<typeof initSqlJs>> | null = null;
	return async () => {
		if (!cached) {
			cached = await initSqlJs({
				wasmBinary: sqlWasmBinary,
			});
		}
		return cached;
	};
}

function toFtsQuery(raw: string): string {
	const parts = raw
		.trim()
		.split(/\s+/)
		.filter(Boolean)
		.map((p) => p.replace(/"/g, "").replace(/\*/g, ""))
		.filter(Boolean);
	if (parts.length === 0) {
		return "";
	}
	return parts.join(" AND ");
}

export default class VaultIndexSearchPlugin extends Plugin {
	settings: VaultIndexSettings = DEFAULT_SETTINGS;
	readonly loadSql = createSqlLoader();

	async onload() {
		await this.loadSettings();
		this.addSettingTab(new VaultIndexSettingTab(this.app, this));
		this.addCommand({
			id: "open-index-search",
			name: "Recherche index (SQLite FTS)",
			callback: () => {
				new SearchModal(this.app, this).open();
			},
		});
	}

	async loadSettings() {
		this.settings = Object.assign(
			{},
			DEFAULT_SETTINGS,
			await this.loadData(),
		);
	}

	async saveSettings() {
		await this.saveData(this.settings);
	}
}

class VaultIndexSettingTab extends PluginSettingTab {
	plugin: VaultIndexSearchPlugin;
	private summaryEl: HTMLElement | null = null;
	private summaryLoading = false;

	constructor(app: App, plugin: VaultIndexSearchPlugin) {
		super(app, plugin);
		this.plugin = plugin;
	}

	display(): void {
		const { containerEl } = this;
		containerEl.empty();
		containerEl.createEl("h2", { text: "Vault index search" });
		new Setting(containerEl)
			.setName("Chemin de la base SQLite")
			.setDesc(
				"Relatif à la racine du vault (ex. .obsidian-index/index.sqlite). Identique à l’option --db du CLI.",
			)
			.addText((t) =>
				t
					.setValue(this.plugin.settings.dbRelativePath)
					.onChange(async (v) => {
						this.plugin.settings.dbRelativePath =
							v.trim() || DEFAULT_SETTINGS.dbRelativePath;
						await this.plugin.saveSettings();
					}),
			);

		new Setting(containerEl)
			.setName("Chemin exécutable tray")
			.setDesc(
				"Optionnel. Chemin absolu vers obsidian-indexer-tray(.exe). Si vide, tentative via PATH.",
			)
			.addText((t) =>
				t
					.setValue(this.plugin.settings.trayExecutablePath)
					.onChange(async (v) => {
						this.plugin.settings.trayExecutablePath = v.trim();
						await this.plugin.saveSettings();
					}),
			);

		new Setting(containerEl)
			.setName("Actions indexeur")
			.setDesc("Lancer le tray, demander un rebuild et lire un bilan rapide.")
			.addButton((b) =>
				b.setButtonText("Lancer tray").onClick(async () => {
					await this.startTray();
				}),
			)
			.addButton((b) =>
				b.setButtonText("Refaire index").onClick(async () => {
					await this.requestRebuild();
				}),
			)
			.addButton((b) =>
				b.setButtonText("Rafraîchir bilan").onClick(async () => {
					await this.refreshSummary();
				}),
			);

		containerEl.createEl("h3", { text: "Bilan index" });
		this.summaryEl = containerEl.createDiv({ cls: "hint" });
		this.summaryEl.setText("Chargement du bilan…");
		void this.refreshSummary();
	}

	private resolveTrayExecutable(): string {
		if (this.plugin.settings.trayExecutablePath.trim()) {
			return this.plugin.settings.trayExecutablePath.trim();
		}
		return Platform.isWin ? "obsidian-indexer-tray.exe" : "obsidian-indexer-tray";
	}

	private async startTray(): Promise<void> {
		try {
			const exe = this.resolveTrayExecutable();
			spawn(exe, [], {
				detached: true,
				stdio: "ignore",
				windowsHide: true,
			}).unref();
			new Notice("Tray lancé (ou demande envoyée).");
		} catch (e) {
			console.error(e);
			new Notice(`Impossible de lancer le tray: ${e}`);
		}
	}

	private async requestRebuild(): Promise<void> {
		try {
			const flag = trayForceRebuildFlagPath();
			mkdirSync(path.dirname(flag), { recursive: true });
			writeFileSync(flag, "rebuild");
			new Notice("Demande de rebuild envoyée au tray.");
		} catch (e) {
			console.error(e);
			new Notice(`Impossible de demander le rebuild: ${e}`);
		}
	}

	private async refreshSummary(): Promise<void> {
		if (!this.summaryEl || this.summaryLoading) {
			return;
		}
		this.summaryLoading = true;
		this.summaryEl.setText("Calcul du bilan…");
		try {
			const exists = await this.app.vault.adapter.exists(
				this.plugin.settings.dbRelativePath,
			);
			if (!exists) {
				this.summaryEl.setText("Base introuvable.");
				return;
			}
			const buf = await this.app.vault.adapter.readBinary(
				this.plugin.settings.dbRelativePath,
			);
			const SQL = await this.plugin.loadSql();
			const db = new SQL.Database(new Uint8Array(buf));
			try {
				const filesByKind = this.queryKindCounts(db);
				const chunks = this.querySingleNumber(db, "SELECT COUNT(*) AS n FROM chunks");
				const annotations = this.querySingleNumber(
					db,
					`SELECT COALESCE(SUM((length(body)-length(replace(body,'[[ANNOTATION]]','')))/14),0) AS n
					   FROM chunks`,
				);
				this.summaryEl.setText(
					`Fichiers: md=${filesByKind.md}, pdf=${filesByKind.pdf}, epub=${filesByKind.epub}, docx=${filesByKind.docx}\n` +
						`Chunks: ${chunks}\n` +
						`Annotations PDF détectées: ${annotations}`,
				);
			} finally {
				db.close();
			}
		} catch (e) {
			console.error(e);
			this.summaryEl.setText(`Erreur bilan: ${e}`);
		} finally {
			this.summaryLoading = false;
		}
	}

	private querySingleNumber(db: Database, sql: string): number {
		const res = db.exec(sql) as Array<{ values: unknown[][] }>;
		if (!res.length || !res[0].values.length) {
			return 0;
		}
		return Number(res[0].values[0][0] ?? 0);
	}

	private queryKindCounts(db: Database): Record<FileKind, number> {
		const out: Record<FileKind, number> = { md: 0, pdf: 0, epub: 0, docx: 0 };
		const res = db.exec(
			"SELECT kind, COUNT(*) AS n FROM files GROUP BY kind ORDER BY kind",
		) as Array<{ values: unknown[][] }>;
		if (!res.length) {
			return out;
		}
		for (const row of res[0].values) {
			const k = String(row[0] ?? "") as FileKind;
			const n = Number(row[1] ?? 0);
			if (k in out) {
				out[k] = n;
			}
		}
		return out;
	}
}

type SearchHit = { path: string; snippet: string };
type SearchQuery = { fts: string; terms: string[] };
type FileKind = "md" | "pdf" | "epub" | "docx";
const ALL_KINDS: FileKind[] = ["md", "pdf", "epub", "docx"];

function parseTerms(raw: string): string[] {
	return raw
		.trim()
		.split(/\s+/)
		.map((p) => p.replace(/"/g, "").replace(/\*/g, "").trim())
		.filter(Boolean);
}

function parseSearchQuery(raw: string): SearchQuery {
	const terms: string[] = [];
	const ftsParts: string[] = [];
	const re = /"([^"]+)"|(\S+)/g;
	for (const m of raw.matchAll(re)) {
		const phrase = m[1]?.trim();
		const word = m[2]?.trim();
		if (phrase) {
			const cleaned = phrase.replace(/\*/g, "");
			if (!cleaned) {
				continue;
			}
			terms.push(cleaned);
			// FTS phrase query. On double les guillemets internes.
			ftsParts.push(`"${cleaned.replace(/"/g, '""')}"`);
			continue;
		}
		if (word) {
			const cleaned = word.replace(/"/g, "").replace(/\*/g, "");
			if (!cleaned) {
				continue;
			}
			terms.push(cleaned);
			ftsParts.push(cleaned);
		}
	}
	return { fts: ftsParts.join(" AND "), terms };
}

function stripPageMarker(text: string): string {
	return text.replace(/\[\[PAGE:\d+\]\]/g, "").trim();
}

function extractPageFromBody(body: string): number | null {
	const m = body.match(/\[\[PAGE:(\d+)\]\]/);
	if (!m) {
		return null;
	}
	const p = Number(m[1]);
	return Number.isFinite(p) && p > 0 ? p : null;
}

function extractSnippetHighlights(snippet: string): string[] {
	const SNIP_OPEN = "__HIT_START__";
	const SNIP_CLOSE = "__HIT_END__";
	const out: string[] = [];
	let rest = snippet;
	while (rest.length > 0) {
		const start = rest.indexOf(SNIP_OPEN);
		const end =
			start >= 0 ? rest.indexOf(SNIP_CLOSE, start + SNIP_OPEN.length) : -1;
		if (start < 0 || end < 0) {
			break;
		}
		const hit = rest.slice(start + SNIP_OPEN.length, end).trim();
		if (hit) {
			out.push(hit);
		}
		rest = rest.slice(end + SNIP_CLOSE.length);
	}
	return out;
}

function cleanChunkBodyForPreview(body: string): string {
	return body
		.replace(/\[\[PAGE:\d+\]\]/g, " ")
		.replace(/\[\[ANNOTATION\]\]/g, " ")
		.replace(/annotation_type:[^\n]+/g, " ")
		.replace(/\s+/g, " ")
		.trim();
}

function buildSnippetFromBody(body: string, terms: string[]): string {
	const cleaned = cleanChunkBodyForPreview(body);
	if (!cleaned) {
		return "";
	}
	if (terms.length === 0) {
		return cleaned.slice(0, 220);
	}
	const lower = cleaned.toLowerCase();
	let best = -1;
	for (const t of terms) {
		const i = lower.indexOf(t.toLowerCase());
		if (i >= 0 && (best < 0 || i < best)) {
			best = i;
		}
	}
	if (best < 0) {
		return cleaned.slice(0, 220);
	}
	const start = Math.max(0, best - 90);
	const end = Math.min(cleaned.length, best + 170);
	const prefix = start > 0 ? "…" : "";
	const suffix = end < cleaned.length ? "…" : "";
	return `${prefix}${cleaned.slice(start, end)}${suffix}`;
}

function trayControlDir(): string {
	const home = process.env.HOME ?? process.env.USERPROFILE ?? "";
	if (Platform.isWin) {
		const appData = process.env.APPDATA ?? path.join(home, "AppData", "Roaming");
		return path.join(appData, "obsidian-indexer");
	}
	if (Platform.isMacOS) {
		return path.join(home, "Library", "Application Support", "obsidian-indexer");
	}
	const xdg = process.env.XDG_CONFIG_HOME ?? path.join(home, ".config");
	return path.join(xdg, "obsidian-indexer");
}

function trayForceRebuildFlagPath(): string {
	return path.join(trayControlDir(), "force-rebuild.flag");
}

class SearchModal extends Modal {
	plugin: VaultIndexSearchPlugin;
	private db: Database | null = null;
	private debounceHandle = 0;

	constructor(app: App, plugin: VaultIndexSearchPlugin) {
		super(app);
		this.plugin = plugin;
	}

	private async ensureDb(): Promise<Database | null> {
		if (this.db) {
			return this.db;
		}
		const rel = this.plugin.settings.dbRelativePath;
		try {
			const exists = await this.app.vault.adapter.exists(rel);
			if (!exists) {
				new Notice(`Base introuvable : ${rel}`);
				return null;
			}
			const buf = await this.app.vault.adapter.readBinary(rel);
			const SQL = await this.plugin.loadSql();
			this.db = new SQL.Database(new Uint8Array(buf));
			return this.db;
		} catch (e) {
			console.error(e);
			new Notice(`Impossible d'ouvrir la base : ${e}`);
			return null;
		}
	}

	async onOpen(): Promise<void> {
		const { contentEl } = this;
		contentEl.empty();
		contentEl.addClass("vault-index-search-modal");

		contentEl.createEl("h2", { text: "Recherche index (plein texte)" });
		contentEl.createEl("p", {
			text: `Base : ${this.plugin.settings.dbRelativePath}`,
			cls: "hint",
		});

		const input = contentEl.createEl("input", {
			type: "text",
			placeholder: "Termes (espaces = AND)",
			cls: "search-input",
		});
		const selectedKinds = new Set<FileKind>(ALL_KINDS);
		const filtersEl = contentEl.createDiv({ cls: "kind-filters" });
		const renderFilters = () => {
			filtersEl.empty();
			for (const kind of ALL_KINDS) {
				const b = filtersEl.createEl("button", {
					text: kind.toUpperCase(),
					cls: selectedKinds.has(kind) ? "kind-chip active" : "kind-chip",
				});
				b.addEventListener("click", () => {
					if (selectedKinds.has(kind)) {
						if (selectedKinds.size > 1) {
							selectedKinds.delete(kind);
						}
					} else {
						selectedKinds.add(kind);
					}
					renderFilters();
					void runSearch();
				});
			}
		};
		renderFilters();

		const resultsEl = contentEl.createDiv({ cls: "results" });

		const runSearch = async () => {
			const parsed = parseSearchQuery(input.value);
			const q = parsed.fts;
			const terms = parsed.terms;
			resultsEl.empty();
			if (!q) {
				resultsEl.setText("Saisissez au moins un mot.");
				return;
			}

			const db = await this.ensureDb();
			if (!db) {
				return;
			}

			try {
				const kinds = [...selectedKinds];
				const placeholders = kinds.map(() => "?").join(", ");
				const stmt = db.prepare(
					`SELECT f.vault_rel_path AS path,
					            f.kind AS kind,
								chunks.body AS body
					   FROM chunks_fts
					   JOIN chunks ON chunks.chunk_id = chunks_fts.rowid
					   JOIN files f ON f.id = chunks.file_id
					  WHERE chunks_fts MATCH ?
					    AND f.kind IN (${placeholders})
					  ORDER BY f.vault_rel_path COLLATE NOCASE, chunks.ordinal
					  LIMIT 80`,
				);
				const hits: Array<
					SearchHit & {
						body: string;
						page: number | null;
						kind: FileKind;
					}
				> = [];
				stmt.bind([q, ...kinds]);
				while (stmt.step()) {
					const row = stmt.getAsObject();
					const body = String(row.body ?? "");
					const kind = String(row.kind ?? "md") as FileKind;
					hits.push({
						path: String(row.path ?? ""),
						snippet: buildSnippetFromBody(body, terms),
						body,
						page: extractPageFromBody(body),
						kind,
					});
				}
				stmt.free();

				if (hits.length === 0) {
					resultsEl.setText("Aucun résultat.");
					return;
				}

				for (const h of hits) {
					const row = resultsEl.createDiv({ cls: "result-row" });
					row.createDiv({ cls: "path", text: h.path });
					const sn = row.createDiv({ cls: "snippet" });
					this.renderHighlightedSnippet(sn, h.snippet, terms);
					if (h.page) {
						row.createDiv({
							cls: "hint",
							text: `page ${h.page}`,
						});
					}
					row.addEventListener("click", () => {
						void this.openVaultPath(h.path, terms, h.page, terms);
						this.close();
					});
				}
			} catch (e) {
				console.error(e);
				resultsEl.setText(`Erreur SQL : ${e}`);
			}
		};

		input.addEventListener("keydown", (ev) => {
			if (ev.key === "Enter") {
				void runSearch();
			}
		});

		input.addEventListener("input", () => {
			window.clearTimeout(this.debounceHandle);
			this.debounceHandle = window.setTimeout(() => {
				void runSearch();
			}, 280);
		});

		await runSearch();
	}

	private renderHighlightedSnippet(
		el: HTMLElement,
		snippet: string,
		terms: string[],
	): void {
		if (!snippet) {
			el.setText("(extrait indisponible)");
			return;
		}
		if (terms.length === 0) {
			el.setText(snippet);
			return;
		}
		const escaped = terms
			.filter(Boolean)
			.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
			.sort((a, b) => b.length - a.length);
		const re = new RegExp(`(${escaped.join("|")})`, "gi");
		let last = 0;
		for (const m of snippet.matchAll(re)) {
			const start = m.index ?? 0;
			const hit = m[0] ?? "";
			if (start > last) {
				el.createSpan({ text: snippet.slice(last, start) });
			}
			el.createSpan({
				text: hit,
				cls: "search-result-file-matched-text",
			});
			last = start + hit.length;
		}
		if (last < snippet.length) {
			el.createSpan({ text: snippet.slice(last) });
		}
	}

	private async openPdfAtPage(file: TFile, page: number): Promise<void> {
		const normalized = normalizePath(file.path);
		const dummyTerm: string | null = null;
		const fragment = this.buildPdfFragment(page, dummyTerm);
		try {
			// Obsidian résout généralement le subpath #page=... ; search est essayé aussi.
			await this.app.workspace.openLinkText(
				`${normalized}${fragment}`,
				normalized,
				false,
			);
			return;
		} catch {
			// fallback ci-dessous
		}
		await this.app.workspace.getLeaf(false).openFile(file);
	}

	private buildPdfFragment(page: number | null, term: string | null): string {
		const parts: string[] = [];
		if (page && Number.isFinite(page) && page > 0) {
			parts.push(`page=${Math.floor(page)}`);
		}
		if (term && term.trim()) {
			parts.push(`search=${encodeURIComponent(term.trim())}`);
		}
		if (parts.length === 0) {
			return "";
		}
		return `#${parts.join("&")}`;
	}

	private pickBestTerm(terms: string[], preferred: string[]): string | null {
		const candidates = [...preferred, ...terms]
			.map((s) => s.trim())
			.filter(Boolean)
			.sort((a, b) => b.length - a.length);
		return candidates[0] ?? null;
	}

	private async revealFirstTermInMarkdown(
		file: TFile,
		terms: string[],
		preferred: string[] = [],
	): Promise<void> {
		const leaf = this.app.workspace.getLeaf(false);
		await leaf.openFile(file);
		const view = leaf.view;
		if (!(view instanceof MarkdownView) || terms.length === 0) {
			return;
		}
		const editor = view.editor;
		const text = editor.getValue();
		const lower = text.toLowerCase();
		const needles = [...preferred, ...terms];
		for (const term of needles) {
			const needle = term.toLowerCase();
			const idx = lower.indexOf(needle);
			if (idx >= 0) {
				const from = editor.offsetToPos(idx);
				const to = editor.offsetToPos(idx + needle.length);
				editor.setSelection(from, to);
				editor.scrollIntoView({ from, to }, true);
				// Prolonge la mise en avant : ouvre la recherche locale sur le terme cliqué.
				this.app.commands.executeCommandById("editor:open-search");
				return;
			}
		}
	}

	async openVaultPath(
		vaultRelPath: string,
		terms: string[] = [],
		page: number | null = null,
		highlights: string[] = [],
	): Promise<void> {
		const normalized = normalizePath(vaultRelPath);
		const f = this.app.vault.getAbstractFileByPath(normalized);
		const bestTerm = this.pickBestTerm(terms, highlights);
		if (f instanceof TFile) {
			if (f.extension.toLowerCase() === "pdf" && page) {
				const fragment = this.buildPdfFragment(page, bestTerm);
				try {
					await this.app.workspace.openLinkText(
						`${normalized}${fragment}`,
						normalized,
						false,
					);
				} catch {
					await this.openPdfAtPage(f, page);
				}
				return;
			}
			if (f.extension.toLowerCase() === "md") {
				await this.revealFirstTermInMarkdown(f, terms, highlights);
				return;
			}
			await this.app.workspace.getLeaf(false).openFile(f);
			return;
		}
		new Notice(`Fichier introuvable dans le vault : ${normalized}`);
	}

	onClose(): void {
		window.clearTimeout(this.debounceHandle);
		try {
			this.db?.close();
		} catch {
			/* ignore */
		}
		this.db = null;
		this.contentEl.empty();
	}
}
