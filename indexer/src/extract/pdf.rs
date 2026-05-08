use anyhow::{Context, Result};
use pdfium_render::prelude::*;
use std::env;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

fn pdfium_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(p) = env::var("OBSIDIAN_INDEXER_PDFIUM_DLL") {
        paths.push(PathBuf::from(p));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(Pdfium::pdfium_platform_library_name()));
        }
    }
    paths.push(Pdfium::pdfium_platform_library_name_at_path("./"));
    paths
}

fn open_pdfium() -> Result<Pdfium> {
    for path in pdfium_search_paths() {
        if path.as_os_str().is_empty() {
            continue;
        }
        if let Ok(bindings) = Pdfium::bind_to_library(&path) {
            return Ok(Pdfium::new(bindings));
        }
    }
    let bindings = Pdfium::bind_to_system_library().with_context(|| {
        "Pdfium introuvable : copiez pdfium.dll dans le même dossier que obsidian-indexer.exe (ou obsidian-indexer-tray.exe), ou définissez OBSIDIAN_INDEXER_PDFIUM_DLL, ou ajoutez Pdfium au PATH système"
    })?;
    Ok(Pdfium::new(bindings))
}

static PDFIUM: OnceLock<Mutex<Option<Result<Pdfium, String>>>> = OnceLock::new();

fn with_pdfium<T>(f: impl FnOnce(&Pdfium) -> Result<T>) -> Result<T> {
    let slot = PDFIUM.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().expect("pdfium lock poisoned");
    if guard.is_none() {
        *guard = Some(match open_pdfium() {
            Ok(pdfium) => Ok(pdfium),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Pdfium non disponible ; les PDF ne seront pas indexés textuellement"
                );
                Err(e.to_string())
            }
        });
    }
    let pdfium = match guard.as_ref().unwrap() {
        Ok(p) => p,
        Err(e) => return Err(anyhow::anyhow!("{}", e)),
    };
    f(pdfium)
}

pub fn extract_pdf(path: &Path) -> Result<String> {
    with_pdfium(|pdfium| {
        let document = pdfium
            .load_pdf_from_file(path, None)
            .with_context(|| format!("charger le PDF {:?}", path))?;

        let mut out = String::new();
        let mut seen = HashSet::<String>::new();

        for (page_index, page) in document.pages().iter().enumerate() {
            push_unique_line(
                &mut out,
                &mut seen,
                &format!("[[PAGE:{}]]", page_index + 1),
            );
            let page_text = page
                .text()
                .with_context(|| format!("pdf page.text {:?}", path))?;

            push_unique_line(&mut out, &mut seen, &page_text.all());

            for annotation in page.annotations().iter() {
                push_unique_line(&mut out, &mut seen, "[[ANNOTATION]]");
                // Types fréquents: Highlight/Underline/Squiggly/Strikeout/Popup/FreeText/Text
                // On collecte plusieurs champs pour maximiser la compatibilité entre producteurs PDF.
                push_unique_line(
                    &mut out,
                    &mut seen,
                    &format!("annotation_type:{:?}", annotation.annotation_type()),
                );
                if let Some(contents) = annotation.contents() {
                    push_unique_line(&mut out, &mut seen, &contents);
                }
                if let Some(name) = annotation.name() {
                    push_unique_line(&mut out, &mut seen, &name);
                }
                if let Some(creator) = annotation.creator() {
                    push_unique_line(&mut out, &mut seen, &creator);
                }
                if let Some(created) = annotation.creation_date() {
                    push_unique_line(&mut out, &mut seen, &created);
                }
                if let Some(modified) = annotation.modification_date() {
                    push_unique_line(&mut out, &mut seen, &modified);
                }

                if let Ok(ann_text) = page_text.for_annotation(&annotation) {
                    push_unique_line(&mut out, &mut seen, &ann_text);
                }
            }
        }

        Ok(out)
    })
}

fn push_unique_line(out: &mut String, seen: &mut HashSet<String>, raw: &str) {
    let s = raw.trim();
    if s.is_empty() {
        return;
    }
    if seen.insert(s.to_string()) {
        out.push_str(s);
        out.push('\n');
    }
}
