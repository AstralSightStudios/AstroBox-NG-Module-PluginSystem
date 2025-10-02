use wasmtime::component::bindgen;

pub mod api;

bindgen!({
    world: "psys-world",
    path: "wit",
});