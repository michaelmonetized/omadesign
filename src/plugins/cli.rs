use super::*;
use std::path::Path;
pub fn cli(args: &[String]) -> Option<Result<(), String>> {
    if let Some(i) = args.iter().position(|a| a == "--install-plugin") {
        return Some((|| {
            let source = args
                .get(i + 1)
                .ok_or("Use --install-plugin FILE_OR_FOLDER")?;
            let p = install(Path::new(source))?;
            println!("Installed {} {} at {}", p.name, p.version, p.path.display());
            Ok(())
        })());
    }
    if args.iter().any(|a| a == "--list-plugins") {
        let catalog = discover();
        for p in catalog.plugins {
            println!(
                "{} {} {}",
                p.id,
                p.version,
                if p.enabled { "enabled" } else { "disabled" }
            );
            for a in p.actions {
                println!("  {} · {} · {}", a.id, a.category, a.name);
            }
        }
        for error in catalog.errors {
            eprintln!("{error}");
        }
        return Some(Ok(()));
    }
    let i = args.iter().position(|a| a == "--plugin")?;
    Some((|| {
        let source = args
            .get(i + 1)
            .ok_or("Use --plugin FILE --command ID --input FILE --output FILE")?;
        let path = Path::new(source);
        let path = if path.is_dir() {
            path.join("main.lua")
        } else {
            path.to_owned()
        };
        let plugin = inspect(&path)?;
        let arg = |name: &str| {
            args.windows(2)
                .find(|a| a[0] == name)
                .map(|a| a[1].as_str())
        };
        let action = arg("--command").ok_or("Choose --command ID")?;
        let parameters = match arg("--params") {
            Some(s) => serde_json::from_str(s).map_err(|e| e.to_string())?,
            None => serde_json::json!({}),
        };
        let inputs = if let Some(dir) = arg("--batch") {
            let mut files = std::fs::read_dir(dir)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .filter(|e| {
                    e.file_type().is_ok_and(|f| f.is_file())
                        && e.path().extension().is_some_and(|x| x == "oma")
                })
                .map(|e| e.path())
                .collect::<Vec<_>>();
            files.sort();
            if files.is_empty() {
                return Err("Batch folder has no .oma documents".into());
            }
            files
        } else {
            vec![PathBuf::from(
                arg("--input").ok_or("Choose --input FILE or --batch DIRECTORY")?,
            )]
        };
        let outdir = arg("--output-dir");
        if args.iter().any(|a| a == "--batch") && outdir.is_none() {
            return Err("Batch processing requires --output-dir DIRECTORY".into());
        }
        if let Some(dir) = outdir {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut failures = vec![];
        for input in inputs {
            let output = if let Some(dir) = outdir {
                Path::new(dir).join(input.file_name().ok_or("Invalid input name")?)
            } else {
                PathBuf::from(arg("--output").ok_or("Choose --output FILE")?)
            };
            let result = (|| {
                if output.exists() {
                    return Err(format!(
                        "Output exists; choose a new path: {}",
                        output.display()
                    ));
                }
                let mut doc = crate::formats::cli::document(&input)?;
                let selection = doc
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| doc.layer_editable(*i))
                    .flat_map(|(i, l)| {
                        l.kind
                            .shapes()
                            .unwrap_or(&[])
                            .iter()
                            .filter(|s| s.visible && !s.locked && !s.guide)
                            .map(move |s| (i, s.id))
                    })
                    .filter(|&(layer, id)| host::shape_editable(&doc, layer, id))
                    .collect();
                let active = (0..doc.layers.len()).find(|&i| doc.layer_editable(i));
                let result = run(
                    &plugin,
                    action,
                    doc.clone(),
                    selection,
                    active,
                    parameters.clone(),
                    None,
                    Arc::new(AtomicBool::new(false)),
                )?;
                if result.brush.is_some() || !result.palettes.is_empty() {
                    return Err("Brush and palette actions run in the desktop Plugins menu; batch commands must edit documents".into());
                }
                crate::document::apply(&mut doc, &Cmd::Batch(result.commands));
                crate::formats::cli::export(&doc, &output)?;
                println!("{} → {}", input.display(), output.display());
                Ok::<_, String>(())
            })();
            if let Err(error) = result {
                failures.push(format!("{}: {error}", input.display()));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    })())
}
