use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::jstring,
};
use shard::mobile::MobileCore;
use std::{path::Path, sync::OnceLock};

static CORE: OnceLock<MobileCore> = OnceLock::new();

// Catch panics before they cross JNI. No pointers or ownership travel across FFI.
fn reply(
    mut env: JNIEnv,
    operation: impl FnOnce(&mut JNIEnv) -> Result<String, String>,
) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(&mut env)));
    let output = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => serde_json::json!({"ok": false, "error": error}).to_string(),
        Err(_) => serde_json::json!({"ok": false, "error": "Native operation failed"}).to_string(),
    };
    match env.new_string(output) {
        Ok(value) => value.into_raw(),
        Err(_) => std::ptr::null_mut(), // JVM already has a pending allocation exception.
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_music_shard_mobile_NativeBridge_initialize(
    env: JNIEnv,
    _: JClass,
    root: JString,
) -> jstring {
    reply(env, |env| {
        let root: String = env.get_string(&root).map_err(|e| e.to_string())?.into();
        if CORE.get().is_none() {
            let core = MobileCore::open(Path::new(&root)).map_err(|e| format!("{e:#}"))?;
            let _ = CORE.set(core);
        }
        Ok("{\"ok\":true,\"value\":null}".into())
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_music_shard_mobile_NativeBridge_dispatch(
    env: JNIEnv,
    _: JClass,
    request: JString,
) -> jstring {
    reply(env, |env| {
        let request: String = env.get_string(&request).map_err(|e| e.to_string())?.into();
        Ok(CORE
            .get()
            .ok_or("Native library not initialized")?
            .dispatch(&request))
    })
}
