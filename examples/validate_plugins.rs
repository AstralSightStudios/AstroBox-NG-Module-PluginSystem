use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use wasmtime::component::Component;
use wasmtime::{Config, Engine};

fn create_engine() -> Result<Engine> {
    let mut config = Config::default();
    config
        .wasm_memory64(false)
        .wasm_component_model(true)
        .wasm_component_model_async(true)
        .epoch_interruption(true);
    Engine::new(&config)
        .map_err(|err| anyhow::Error::from(err.context("failed to create Wasmtime 46 engine")))
}

fn validate_component(engine: &Engine, path: &Path) -> Result<()> {
    let wasm = std::fs::read(path)
        .with_context(|| format!("failed to read component {}", path.display()))?;
    let component = Component::from_binary(engine, &wasm).map_err(|err| {
        anyhow::Error::from(err.context(format!("failed to compile component {}", path.display())))
    })?;
    if std::env::var_os("PRINT_IMPORTS").is_some() {
        for (name, import) in component.component_type().imports(engine) {
            println!("IMPORT\t{}\t{}", path.display(), name);
            if let wasmtime::component::types::ComponentItem::ComponentInstance(instance) =
                import.ty
            {
                for (function, export) in instance.exports(engine) {
                    if let wasmtime::component::types::ComponentItem::ComponentFunc(func) =
                        export.ty
                    {
                        println!("FUNC\t{}\t{}\t{}", name, function, func.async_());
                    }
                }
            }
        }
    }
    let precompiled = engine.precompile_component(&wasm).map_err(|err| {
        anyhow::Error::from(
            err.context(format!("failed to precompile component {}", path.display())),
        )
    })?;
    let _ = unsafe { Component::deserialize(engine, precompiled) }.map_err(|err| {
        anyhow::Error::from(err.context(format!(
            "failed to deserialize component {}",
            path.display()
        )))
    })?;
    Ok(())
}

fn main() -> Result<()> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("usage: validate_plugins <plugin-directory>")?,
    );
    let engine = create_engine()?;
    let mut failures = Vec::new();
    let mut count = 0;

    for entry in std::fs::read_dir(&root)
        .with_context(|| format!("failed to read plugin directory {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        for wasm in std::fs::read_dir(&path)? {
            let wasm = wasm?.path();
            if wasm.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
                continue;
            }
            count += 1;
            match validate_component(&engine, &wasm) {
                Ok(()) => println!("OK\t{}", wasm.display()),
                Err(err) => {
                    eprintln!("FAIL\t{}\t{err:#}", wasm.display());
                    failures.push(wasm);
                }
            }
        }
    }

    if failures.is_empty() {
        println!("validated {count} downloaded plugin component(s)");
        Ok(())
    } else {
        Err(anyhow!(
            "{} of {count} plugin component(s) failed validation",
            failures.len()
        ))
    }
}
