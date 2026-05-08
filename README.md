# obsidian-indexer

Chaîne d’outils pour indexer un coffre Obsidian dans une base **SQLite** avec recherche plein texte (**FTS4**, compatible avec **sql.js** dans Obsidian), via le plugin **vault-index-search**.

## Composants

| Élément | Rôle |
|--------|------|
| **Indexeur** (`indexer/`) | CLI Rust : scan du vault, extraction MD / DOCX / EPUB / PDF, chunks + hash, écriture `index.sqlite`. Option **tray** (surveillance + zone de notification). |
| **Plugin** (`plugin/`) | Extension Obsidian : recherche FTS dans la base SQLite (via sql.js). |

La base peut être synchronisée avec le vault (Syncthing, Drive, git, etc.) pour garder la même recherche sur mobile ou un autre poste.

Les bases créées avec une ancienne version (**schéma v1**, index FTS5 côté Rust uniquement) sont **migrées automatiquement en v2 (FTS4)** au prochain passage de l’indexeur, pour rester compatibles avec **sql.js** dans Obsidian.

## Prérequis

- **Rust** (stable) — [rustup](https://rustup.rs/)
- **Node.js** LTS — pour compiler le plugin (`npm`)

Pour l’extraction **PDF**, l’indexeur charge **Pdfium** dynamiquement : il cherche `pdfium.dll` (Windows) **d’abord dans le dossier de l’exécutable**, puis le répertoire courant, puis la bibliothèque système. Variable optionnelle **`OBSIDIAN_INDEXER_PDFIUM_DLL`** : chemin complet vers la DLL. Sans Pdfium valide, les PDF ne sont pas indexés textuellement ; le reste du coffre l’est toujours.

## Build tout-en-un (Windows)

À la racine du dépôt :

```powershell
.\build.ps1
```

Ce script :

1. **`cargo fetch`** puis **`cargo build`** du crate `obsidian-indexer` avec la fonctionnalité **`tray`** (CLI + `obsidian-indexer-tray`).
2. Dans **`plugin/`** : **`npm ci`** (si `package-lock.json` existe) puis **`npm run build`** (esbuild ; le WASM sql.js est **embarqué dans `main.js`**).

Options utiles :

```powershell
.\build.ps1 -SkipPlugin       # uniquement Rust
.\build.ps1 -SkipRust         # uniquement plugin
.\build.ps1 -NoTray           # CLI seul (sans binaire tray)
.\build.ps1 -DebugBuild        # Rust en debug (sans --release)
```

Par défaut : **release** et **feature tray** activées.

Les binaires Rust sont dans **`target/release/`** (depuis la racine du workspace). Le plugin livrable est le dossier **`plugin/`** après build : au minimum **`main.js`**, **`manifest.json`**, **`styles.css`** si présent — **aucun fichier `.wasm` séparé** n'est nécessaire.

Build manuel :

```bash
cd indexer && cargo build --release -p obsidian-indexer --features tray
cd ../plugin && npm ci && npm run build
```

## Installer le plugin dans Obsidian

Copier le dossier **`plugin`** (tel quel après build) vers :

`<vault>/.obsidian/plugins/vault-index-search/`

Activer « Vault Index Search » dans les extensions Obsidian et configurer le chemin vers `index.sqlite` si besoin (voir les paramètres du plugin).

## Tray + plugin (intégration actuelle)

- Le binaire **`obsidian-indexer-tray`** fournit :
  - démarrage/arrêt rapide via icône de zone de notification ;
  - fenêtre **Configuration** ;
  - fenêtre **Bilan** (compteurs indexés/ignorés/erreurs par type) ;
  - fenêtre **Logs**.
- Le plugin peut aussi, depuis ses paramètres :
  - lancer le tray (si l’exécutable est dans le `PATH` ou si un chemin explicite est renseigné) ;
  - demander un **rebuild forcé** de l’index ;
  - afficher un bilan rapide (fichiers/chunks/annotations PDF).

## Recherche (plugin)

- Recherche plein texte FTS4 (compatible `sql.js`).
- Requêtes avec guillemets prises en charge pour les phrases (ex. `"charge de ses besoins"`).
- Filtres rapides par format : `md`, `pdf`, `epub`, `docx`.
- Navigation au clic :
  - markdown : ouverture + sélection du terme ;
  - PDF : ouverture à la bonne page quand l’information est disponible.

## Limitations connues

- Les PDFs nécessitent une bibliothèque **Pdfium** disponible (`pdfium.dll` sur Windows).
- Certaines annotations PDF peuvent varier selon le producteur PDF (Adobe, Zotero, etc.) ; l’extraction actuelle couvre les champs principaux mais peut encore être améliorée.
- Le surlignage interne du terme dans le viewer PDF Obsidian dépend des capacités d’URL/fragment du viewer.

## Roadmap

- [ ] **Multilingue** (UI plugin + tray, messages FR/EN, i18n)
- [ ] **Filtres plus précis** (ex. annotations seules, contenu principal seul, types d’annotations)
- [ ] **Plus de types de fichiers indexés** (ex. txt, html, csv, odt, pptx…)
- [ ] **Amélioration de l’interface** (plugin + tray) et **tests mobile** (performances, UX, compatibilité)
- [ ] Historique du bilan (dernier N passages avec tendances)
- [ ] Diagnostic “fichier problématique” (logs ciblés par document)
- [ ] Mode “priorité récence” / pondération configurable des résultats
- [ ] Export/import de configuration (plugin + tray)

## Développement et CI

Les workflows GitHub Actions dans **`.github/workflows/`** exécutent les tests Rust et la build du plugin. Consulter ces fichiers pour les commandes exactes utilisées en intégration continue.
