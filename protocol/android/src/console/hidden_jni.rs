//! Additional `NightjarConsoleJni` exports for explicit favourites, `hasLibrary`, and classic
//! `HiddenStore` parity (`of`, `ids`, `setApp`, `following`, `changed`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{JNI_FALSE, jboolean, jint, jlong, jstring};
use nightjar_core::console::{Favourite, find_cached_library_cover, has_library};

use crate::http::{jbool, new_jstring};

use super::jni::{REPOS, jstr};

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHasLibrary(
    mut env: JNIEnv,
    _: JClass,
    base_dir: JString,
    host_id: JString,
) -> jboolean {
    let Some(dir) = jstr(&mut env, &base_dir) else {
        return JNI_FALSE;
    };
    let Some(hid) = jstr(&mut env, &host_id) else {
        return JNI_FALSE;
    };
    jbool(has_library(&PathBuf::from(dir), &hid))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoSetFavourite(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    entry_json: JString,
    starred: jboolean,
) -> jboolean {
    let Some(entry) = REPOS.get(handle) else {
        return JNI_FALSE;
    };
    let Some(fav) =
        jstr(&mut env, &entry_json).and_then(|s| serde_json::from_str::<Favourite>(&s).ok())
    else {
        crate::platform::error("repoSetFavourite: malformed favourite JSON");
        return JNI_FALSE;
    };
    if let Err(e) = entry.repo.set_favourite_explicit(fav, starred != JNI_FALSE) {
        crate::platform::error(&format!("repoSetFavourite failed: {e}"));
        return JNI_FALSE;
    }
    crate::policy::jbool(true)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoIsFavourite(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
    install_id: JString,
    app_id: jint,
) -> jboolean {
    let Some(entry) = REPOS.get(handle) else {
        return JNI_FALSE;
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let iid = jstr(&mut env, &install_id).unwrap_or_default();
    let app = u32::try_from(app_id).unwrap_or(0);
    jbool(entry.repo.is_favourite(&hid, &iid, app))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHiddenOf(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
) -> jstring {
    let Some(entry) = REPOS.get(handle) else {
        return new_jstring(&env, "[]");
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let ids = entry.repo.hidden_store().of_app_ids(&hid);
    new_jstring(
        &env,
        &serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHiddenIds(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
) -> jstring {
    let Some(entry) = REPOS.get(handle) else {
        return new_jstring(&env, "{}");
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let ids = entry.repo.hidden_store().stable_ids(&hid);
    new_jstring(
        &env,
        &serde_json::to_string(&ids).unwrap_or_else(|_| "{}".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHiddenSetApp(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
    app_id: jint,
    stable_id: JString,
    hidden: jboolean,
) -> jboolean {
    let Some(entry) = REPOS.get(handle) else {
        return JNI_FALSE;
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let sid = jstr(&mut env, &stable_id).unwrap_or_default();
    let app = u32::try_from(app_id).unwrap_or(0);
    if let Err(e) = entry
        .repo
        .hidden_store()
        .set_app(&hid, app, &sid, hidden != JNI_FALSE)
    {
        crate::platform::error(&format!("repoHiddenSetApp failed: {e}"));
        return JNI_FALSE;
    }
    crate::policy::jbool(true)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHiddenFollowing(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
    games_json: JString,
) -> jstring {
    let Some(entry) = REPOS.get(handle) else {
        return new_jstring(&env, "[]");
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let games: BTreeMap<String, u32> = jstr(&mut env, &games_json)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let ids = match entry.repo.hidden_store().following(&hid, &games) {
        Ok(ids) => ids,
        Err(e) => {
            crate::platform::error(&format!("repoHiddenFollowing failed: {e}"));
            Default::default()
        }
    };
    new_jstring(
        &env,
        &serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoHiddenChanged(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    host_id: JString,
    shown_json: JString,
    hidden_json: JString,
) -> jstring {
    let Some(entry) = REPOS.get(handle) else {
        return new_jstring(&env, "[]");
    };
    let hid = jstr(&mut env, &host_id).unwrap_or_default();
    let shown: Vec<u32> = jstr(&mut env, &shown_json)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let hidden: Vec<u32> = jstr(&mut env, &hidden_json)
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let ids = match entry.repo.hidden_store().changed(&hid, &shown, &hidden) {
        Ok(ids) => ids,
        Err(e) => {
            crate::platform::error(&format!("repoHiddenChanged failed: {e}"));
            Default::default()
        }
    };
    new_jstring(
        &env,
        &serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoRecents(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jstring {
    let Some(entry) = REPOS.get(handle) else {
        return new_jstring(&env, "[]");
    };
    let recents = entry.repo.recents_store().list_stored();
    new_jstring(
        &env,
        &serde_json::to_string(&recents).unwrap_or_else(|_| "[]".into()),
    )
}

pub(super) fn find_cached_library_cover_json(
    base_dir: &str,
    host_id: &str,
    app_id: jint,
    install_id: &str,
) -> Option<String> {
    let app = u32::try_from(app_id).unwrap_or(0);
    let cover = find_cached_library_cover(Path::new(base_dir), host_id, app, install_id)?;
    serde_json::to_string(&cover).ok()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarConsoleJni_repoFindCachedLibraryCover(
    mut env: JNIEnv,
    _: JClass,
    base_dir: JString,
    host_id: JString,
    app_id: jint,
    install_id: JString,
) -> jstring {
    let Some(dir) = jstr(&mut env, &base_dir) else {
        return std::ptr::null_mut();
    };
    let Some(hid) = jstr(&mut env, &host_id) else {
        return std::ptr::null_mut();
    };
    let iid = jstr(&mut env, &install_id).unwrap_or_default();
    match find_cached_library_cover_json(&dir, &hid, app_id, &iid) {
        Some(json) => new_jstring(&env, &json),
        None => std::ptr::null_mut(),
    }
}
