//! End-to-end check of the Level 4 `mass` / `queue` / `resource-install` WIT against real guest
//! components (built with wit-bindgen for the real worlds) running on wasmtime 48.
//! The host side mirrors the shapes in `pluginsystem/src/v4/host/{mass,queue}.rs`.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use wasmtime::component::{Accessor, Component, HasData, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod harness {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "harness",
        imports: {
            "astrobox:psys-host-v4/mass.send": async | store | trappable,
            "astrobox:psys-host-v4/queue.enqueue-install": async | store | trappable,
            "astrobox:psys-host-v4/queue.cancel-task": async | store | trappable,
        },
        exports: { default: async | store },
    });
}

// The same lookup the real runtime does on an already-instantiated component.
mod export_only {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "psys-plugin-resource-install-export",
        exports: { default: async | store },
    });
}

use harness::astrobox::psys_host_v4::{mass, queue};
use harness::exports::astrobox::psys_plugin_v4::resource_install::{InstallOutcome, InstallRequest};

#[derive(Default)]
struct Recorded {
    mass: Vec<(String, String, Vec<u8>, Option<String>, Option<String>, Option<String>)>,
    tasks: Vec<String>,
    cancelled: Vec<String>,
}

struct State {
    wasi: WasiCtx,
    table: ResourceTable,
    rec: Arc<Mutex<Recorded>>,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}
impl HasData for State {
    type Data<'a> = &'a mut State;
}

impl mass::Host for State {}
impl mass::HostWithStore<State> for State {
    async fn send(
        accessor: &Accessor<State, Self>,
        device_addr: String,
        data_type: mass::DataType,
        data: Vec<u8>,
        options: mass::SendOptions,
    ) -> wasmtime::Result<Result<(), String>> {
        let rec = accessor.with(|mut a| Arc::clone(&a.get().rec));
        if device_addr == "SLOW" {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if device_addr == "ERR" {
            return Ok(Err("device busy".to_string()));
        }
        rec.lock().unwrap().mass.push((
            device_addr,
            format!("{data_type:?}"),
            data,
            options.watchface_id,
            options.package_name,
            options.progress_event,
        ));
        Ok(Ok(()))
    }
}

impl queue::Host for State {
    fn add_resource_to_queue(&mut self, _t: queue::ResourceType, _p: String) {}
}
impl queue::HostWithStore<State> for State {
    async fn enqueue_install(
        accessor: &Accessor<State, Self>,
        task: queue::InstallTask,
    ) -> wasmtime::Result<Result<String, String>> {
        let rec = accessor.with(|mut a| Arc::clone(&a.get().rec));
        if task.device_addr == "ERR" {
            return Ok(Err("device offline".to_string()));
        }
        let source = match &task.source {
            queue::InstallSource::File(p) => format!("file:{p}"),
            queue::InstallSource::Bytes(b) => format!("bytes:{b:?}"),
        };
        rec.lock().unwrap().tasks.push(format!(
            "name={} desc={:?} icon={:?} dev={} type={:?} src={} wf={:?}",
            task.name, task.description, task.icon, task.device_addr, task.res_type, source, task.watchface_id
        ));
        Ok(Ok("plugin-task-1".to_string()))
    }
    async fn cancel_task(accessor: &Accessor<State, Self>, task_id: String) -> wasmtime::Result<Result<(), String>> {
        let rec = accessor.with(|mut a| Arc::clone(&a.get().rec));
        rec.lock().unwrap().cancelled.push(task_id);
        Ok(Ok(()))
    }
}

fn engine() -> Result<Engine> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.wasm_component_model_async(true);
    Ok(Engine::new(&config)?)
}

fn make_linker(engine: &Engine, component: &Component) -> Result<Linker<State>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    wasmtime_wasi::p3::add_to_linker(&mut linker)?;
    harness::Harness::add_to_linker::<State, State>(&mut linker, |s| s)?;
    linker.define_unknown_imports_as_traps(component)?;
    Ok(linker)
}

fn request(data: &str) -> InstallRequest {
    InstallRequest {
        request_id: "req-1".into(),
        provider: "OfficialV2".into(),
        resource_id: "res-中".into(),
        resource_name: "Cool Face 表盘".into(),
        resource_type: "watchface".into(),
        download_key: "p67".into(),
        version: "1.2.0".into(),
        version_code: Some(12),
        device_addr: "AA:BB:CC".into(),
        is_update: true,
        data: data.into(),
    }
}

fn describe(outcome: &InstallOutcome) -> String {
    match outcome {
        InstallOutcome::Queued => "queued".into(),
        InstallOutcome::Redeem(m) => format!("redeem:{m}"),
        InstallOutcome::Pass => "pass".into(),
        InstallOutcome::Failed(m) => format!("failed:{m}"),
    }
}

struct Checks {
    passed: usize,
    failed: Vec<String>,
}
impl Checks {
    fn check(&mut self, name: &str, ok: bool, detail: impl FnOnce() -> String) {
        if ok {
            self.passed += 1;
            println!("  ok   {name}");
        } else {
            let d = detail();
            println!("  FAIL {name}: {d}");
            self.failed.push(format!("{name}: {d}"));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let dir = std::env::args().nth(1).ok_or_else(|| anyhow!("usage: e2e-host <dir with guest_ri.wasm guest_base.wasm>"))?;
    let engine = engine()?;
    let mut c = Checks { passed: 0, failed: vec![] };

    // ---- the component that implements resource-install --------------------------------------
    let component = Component::from_file(&engine, format!("{dir}/guest_ri.wasm"))?;
    let rec = Arc::new(Mutex::new(Recorded::default()));
    let mut store = Store::new(
        &engine,
        State { wasi: WasiCtxBuilder::new().build(), table: ResourceTable::new(), rec: Arc::clone(&rec) },
    );
    let linker = make_linker(&engine, &component)?;
    let instance = linker.instantiate_pre(&component)?.instantiate_async(&mut store).await?;

    // The same fallible lookup the real driver does
    let export = export_only::PsysPluginResourceInstallExport::new(&mut store, &instance);
    c.check("export lookup succeeds for a plugin that implements resource-install", export.is_ok(), || {
        format!("{:?}", export.as_ref().err())
    });
    let export = export?;

    let world = harness::Harness::new(&mut store, &instance)?;
    let handler = world.astrobox_psys_plugin_v4_resource_install();

    let cases: Vec<&str> = vec![
        "echo", "queued", "pass", "redeem", "redeem-empty", "fail", "enqueue-file", "enqueue-bytes",
        "enqueue-err", "mass-all", "mass-err", "unknown",
    ];
    let outcomes: Vec<(String, String)> = store
        .run_concurrent(async |accessor| {
            let mut out = Vec::new();
            for data in &cases {
                let o = handler.call_on_install(accessor, request(data)).await?;
                out.push((data.to_string(), describe(&o)));
            }
            anyhow::Ok(out)
        })
        .await??;
    let by = |k: &str| outcomes.iter().find(|(d, _)| d == k).map(|(_, o)| o.clone()).unwrap_or_default();

    let echo = by("echo");
    c.check(
        "request fields cross the ABI intact (unicode, option<u32>, bool, strings)",
        echo == "failed:id=req-1 provider=OfficialV2 res=res-中/Cool Face 表盘 type=watchface key=p67 ver=1.2.0 code=Some(12) dev=AA:BB:CC upd=true data=echo",
        || echo.clone(),
    );
    c.check("queued outcome", by("queued") == "queued", || by("queued"));
    c.check("pass outcome", by("pass") == "pass", || by("pass"));
    c.check("redeem(message) outcome keeps the unicode message", by("redeem") == "redeem:请先输入兑换码", || by("redeem"));
    c.check("redeem with an empty message", by("redeem-empty") == "redeem:", || by("redeem-empty"));
    c.check("failed(message) outcome keeps the unicode message", by("fail") == "failed:boom 失败", || by("fail"));
    c.check("a plugin returning a non-listed branch is still just an outcome", by("unknown") == "failed:unknown data", || by("unknown"));

    c.check("enqueue-install (file) + cancel-task round trip through the plugin", by("enqueue-file") == "redeem:task=plugin-task-1", || by("enqueue-file"));
    c.check("enqueue-install (bytes) round trip", by("enqueue-bytes") == "redeem:task=plugin-task-1", || by("enqueue-bytes"));
    c.check("enqueue-install error string reaches the plugin", by("enqueue-err") == "failed:device offline", || by("enqueue-err"));

    let r = rec.lock().unwrap();
    c.check("enqueue-install task fields (option/variant/enum) arrive intact: file", r.tasks.first().map(String::as_str)
        == Some("name=Cool Face 表盘 desc=Some(\"来自测试插件\") icon=Some(\"https://example.com/i.png\") dev=AA:BB:CC type=ResourceType::Watchface src=file:downloads/face.bin wf=Some(\"12345\")"),
        || format!("{:?}", r.tasks.first()));
    c.check("enqueue-install task fields arrive intact: bytes", r.tasks.get(1).map(String::as_str)
        == Some("name=Cool Face 表盘 desc=Some(\"来自测试插件\") icon=Some(\"https://example.com/i.png\") dev=AA:BB:CC type=ResourceType::Watchface src=bytes:[1, 2, 3, 250, 0, 255] wf=Some(\"12345\")"),
        || format!("{:?}", r.tasks.get(1)));
    c.check("cancel-task receives the id the host returned", r.cancelled == vec!["plugin-task-1".to_string(), "plugin-task-1".to_string()], || format!("{:?}", r.cancelled));

    c.check("mass-all outcome", by("mass-all") == "queued", || by("mass-all"));
    let kinds: Vec<&str> = r.mass.iter().map(|m| m.1.as_str()).collect();
    c.check("mass.send data-type enum covers all four kinds in order", kinds == ["DataType::Watchface", "DataType::Quickapp", "DataType::Firmware", "DataType::NotificationIcon"], || format!("{kinds:?}"));
    c.check("mass.send payload and options arrive intact", r.mass.iter().all(|m| m.0 == "AA:BB:CC" && m.2 == vec![9, 8, 7]
        && m.3.as_deref() == Some("wf1") && m.4.as_deref() == Some("com.example.app") && m.5.as_deref() == Some("my.progress")), || format!("{:?}", r.mass));
    c.check("mass.send error string reaches the plugin", by("mass-err") == "failed:device busy", || by("mass-err"));
    drop(r);

    // ---- concurrency: one slow host call must not block another in-flight on-install --------------
    let started = Instant::now();
    let (slow, quick) = store
        .run_concurrent(async |accessor| {
            let slow = async {
                let o = handler.call_on_install(accessor, request("slow")).await?;
                anyhow::Ok((describe(&o), started.elapsed()))
            };
            let quick = async {
                let o = handler.call_on_install(accessor, request("pass")).await?;
                anyhow::Ok((describe(&o), started.elapsed()))
            };
            let (slow, quick) = futures::future::join(slow, quick).await;
            anyhow::Ok((slow?, quick?))
        })
        .await??;
    c.check(
        "slow mass.send (300ms) in one on-install does not block a concurrent on-install",
        slow.0 == "queued" && quick.0 == "pass" && slow.1 >= Duration::from_millis(290) && quick.1 < Duration::from_millis(150),
        || format!("slow={slow:?} quick={quick:?}"),
    );

    // ---- a plugin that does NOT implement resource-install --------------------------------------
    let base = Component::from_file(&engine, format!("{dir}/guest_base.wasm"))?;
    let mut store2 = Store::new(
        &engine,
        State { wasi: WasiCtxBuilder::new().build(), table: ResourceTable::new(), rec: Arc::new(Mutex::new(Recorded::default())) },
    );
    let linker2 = make_linker(&engine, &base)?;
    let instance2 = linker2.instantiate_pre(&base)?.instantiate_async(&mut store2).await?;
    let missing = export_only::PsysPluginResourceInstallExport::new(&mut store2, &instance2);
    c.check("export lookup fails (fallibly) for a plugin without resource-install, instantiation is fine", missing.is_err(), || "lookup unexpectedly succeeded".into());
    let _ = export;

    println!("\n{} passed, {} failed", c.passed, c.failed.len());
    if !c.failed.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
