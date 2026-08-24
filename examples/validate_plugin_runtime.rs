use anyhow::{Context, Result, anyhow};
use pluginsystem::manifest::PluginManifest;
use pluginsystem::plugin::PluginRuntime;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<()> {
    let roots = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        anyhow::bail!("usage: validate_plugin_runtime <plugin-directory>...");
    }

    let app = tauri::Builder::<tauri::Wry>::default()
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .context("failed to create a Wry test app")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to create validation runtime")?;

    let mut failures = Vec::new();
    for root in roots {
        let directories = if root.join("manifest.json").is_file() {
            vec![root]
        } else {
            std::fs::read_dir(&root)
                .with_context(|| format!("failed to read plugin directory {}", root.display()))?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|path| path.is_dir())
                .collect()
        };

        for path in directories {
            let manifest = match PluginManifest::load_from_dir(&path) {
                Ok(manifest) => manifest,
                Err(err) => {
                    eprintln!("FAIL\t{}\t{err:#}", path.display());
                    failures.push(path);
                    continue;
                }
            };
            let result = PluginRuntime::initialise(&path, &manifest, app.handle().clone())
                .and_then(|plugin| {
                    runtime.block_on(async {
                        tokio::time::timeout(Duration::from_secs(30), plugin.run())
                            .await
                            .context("plugin runtime validation timed out after 30 seconds")?
                    })
                });
            match result {
                Ok(()) => println!("OK\t{}", manifest.name),
                Err(err) => {
                    eprintln!("FAIL\t{}\t{err:#}", path.display());
                    failures.push(path);
                }
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "{} plugin(s) failed runtime validation",
            failures.len()
        ))
    }
}
