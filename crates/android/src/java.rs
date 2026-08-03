use crate::JVM;
use jni::errors::LogErrorAndDefault;
use jni::objects::{JByteArray, JObject, JString};
use jni::strings::JNIStr;
use jni::{jni_sig, jni_str, Env, EnvUnowned, JValue};

const CLASS_NAME: &JNIStr = jni_str!("com/mxmgorin/oxgbc/MainActivity");

/// Call this to show the file picker
pub fn show_android_file_picker() {
    let mut env = unsafe { EnvUnowned::from_raw(crate::sdl_jni_env_ptr()) };
    env.with_env(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, crate::sdl_activity_raw()) };
        env.call_method(&activity, jni_str!("openFilePicker"), jni_sig!("()V"), &[])?;
        Ok(())
    })
    .resolve::<LogErrorAndDefault>();
}

/// Call this to show the directory picker
pub fn show_android_directory_picker() {
    let mut env = unsafe { EnvUnowned::from_raw(crate::sdl_jni_env_ptr()) };
    env.with_env(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, crate::sdl_activity_raw()) };
        env.call_method(
            &activity,
            jni_str!("openDirectoryPicker"),
            jni_sig!("()V"),
            &[],
        )?;
        Ok(())
    })
    .resolve::<LogErrorAndDefault>();
}

/// Reads bytes from an Android content:// URI using Java's ContentResolver
pub fn read_uri_bytes(uri: &str) -> Option<Vec<u8>> {
    let vm = JVM.get()?;
    vm.attach_current_thread(|env| -> jni::errors::Result<Vec<u8>> {
        let class = env.find_class(CLASS_NAME)?;
        let j_str = env.new_string(uri)?;
        let j_obj: JObject = j_str.into();

        let result = env
            .call_static_method(
                &class,
                jni_str!("readUriBytes"),
                jni_sig!("(Ljava/lang/String;)[B"),
                &[JValue::Object(&j_obj)],
            )?
            .l()?;

        if result.is_null() {
            return Err(jni::errors::Error::NullPtr("readUriBytes returned null"));
        }

        let array = env.cast_local::<JByteArray>(result)?;
        env.convert_byte_array(&array)
    })
    .ok()
}

pub fn file_name(uri: &str) -> Option<String> {
    let vm = JVM.get()?;
    vm.attach_current_thread(|env| -> jni::errors::Result<Option<String>> {
        let class = env.find_class(CLASS_NAME)?;
        let j_str = env.new_string(uri)?;
        let j_obj: JObject = j_str.into();

        let result = env
            .call_static_method(
                &class,
                jni_str!("getFileName"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/String;"),
                &[JValue::Object(&j_obj)],
            )?
            .l()?;

        if result.is_null() {
            return Ok(None);
        }

        let j_string = env.cast_local::<JString>(result)?;
        Ok(Some(j_string.try_to_string(env)?))
    })
    .ok()
    .flatten()
}

/// Call Java method getFilesInDirectory(String uri, String[] extensions)
pub fn read_dir(uri: &str) -> Option<Vec<String>> {
    let vm = JVM.get()?;
    vm.attach_current_thread(|env| -> jni::errors::Result<Vec<String>> {
        let class = env.find_class(CLASS_NAME)?;
        let j_str = env.new_string(uri)?;
        let j_obj: JObject = j_str.into();

        let list = env
            .call_static_method(
                &class,
                jni_str!("getFilesInDirectory"),
                jni_sig!("(Ljava/lang/String;)Ljava/util/List;"),
                &[JValue::Object(&j_obj)],
            )?
            .l()?;

        j_obj_to_vec(env, &list)
    })
    .ok()
}

fn j_obj_to_vec(env: &mut Env, list: &JObject) -> jni::errors::Result<Vec<String>> {
    let size = env
        .call_method(list, jni_str!("size"), jni_sig!("()I"), &[])?
        .i()?;
    let mut vec = Vec::with_capacity(size.max(0) as usize);

    for i in 0..size {
        let element = env
            .call_method(
                list,
                jni_str!("get"),
                jni_sig!("(I)Ljava/lang/Object;"),
                &[JValue::Int(i)],
            )?
            .l()?;

        let j_string = env.cast_local::<JString>(element)?;
        vec.push(j_string.try_to_string(env)?);
    }

    Ok(vec)
}
