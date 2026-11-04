//! Google Play Games Services on Android, the SDK's Java classes called through JNI.
//!
//! The game has no Java of its own: `android_dependencies` in `Cargo.toml` makes
//! `cargo love2drs` package the SDK. Without that the classes aren't there, and this
//! quietly turns itself off, leaving local trophies. See `docs/achievements.md`.
//!
//! Everything runs on one background thread, because the SDK answers with `Task`s
//! and `Tasks.await` is the way to wait for one without writing Java (it refuses to
//! run on the main thread). A thread attached to the VM only sees the system's
//! classes through JNI's `FindClass`, so the SDK's come from the activity's loader.
//!
//! The activity has to be the real one, from `love2drs::android_app`. `ndk_context` holds
//! the `Application`, and passing that to the SDK's `Activity` arguments aborts the process.
//!
//! The SDK signs the player in by itself at launch, so most of the time they
//! never see this. A player who declined can sign in with the button on the
//! trophy shelf ([`sign_in`]), which Google's guidelines ask for.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use jni::objects::{JClass, JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};

use crate::euchre::record::Achievement;

/// Play Console makes up an id for each achievement. Copy them from
/// Grow users > Play Games Services > Setup and management > Achievements > Get
/// resources. A trophy that isn't listed stays local.
const IDS: &[(Achievement, &str)] = &[
    // (Achievement::FirstGame, "CgkI..."),
];

/// `Achievement.STATE_UNLOCKED`.
const STATE_UNLOCKED: i32 = 0;

enum Command {
    Report(Vec<Achievement>),
    SignIn,
}

static COMMANDS: OnceLock<Sender<Command>> = OnceLock::new();
/// The SDK is in this build and answered.
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static SIGNED_IN: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let (send, receive) = channel();
    if COMMANDS.set(send).is_err() {
        return;
    }
    let started = std::thread::Builder::new().name("play-games".into()).spawn(move || {
        if let Err(e) = run(receive) {
            crate::assets::log_warn(&format!("Play Games is off: {e}"));
        }
    });
    if let Err(e) = started {
        crate::assets::log_warn(&format!("Play Games is off: {e}"));
    }
}

pub fn signed_in() -> bool {
    SIGNED_IN.load(Ordering::Relaxed)
}

/// Whether a sign-in button would do anything.
pub fn can_sign_in() -> bool {
    AVAILABLE.load(Ordering::Relaxed) && !signed_in()
}

pub fn sign_in() {
    if let Some(commands) = COMMANDS.get() {
        let _ = commands.send(Command::SignIn);
    }
}

/// Sends these trophies now, or as soon as the player is signed in.
pub fn report(list: &[Achievement]) {
    if let Some(commands) = COMMANDS.get() {
        let _ = commands.send(Command::Report(list.to_vec()));
    }
}

fn play_id(a: Achievement) -> Option<&'static str> {
    IDS.iter().find(|(known, _)| *known == a).map(|(_, id)| *id)
}

fn from_play_id(id: &str) -> Option<Achievement> {
    IDS.iter().find(|(_, known)| *known == id).map(|(a, _)| *a)
}

fn run(commands: Receiver<Command>) -> Result<(), String> {
    let app = love2drs::android_app();
    // SAFETY: both pointers are valid here, during `main`, and a `JObject` doesn't
    // delete what it holds. Our own reference then outlives the activity, because
    // this thread may still be running in the moment before the process exits.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread_permanently().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let activity = env.new_global_ref(activity).map_err(|e| e.to_string())?;
    let play = Play::new(&mut env, activity.as_obj()).map_err(|e| {
        clear_exception(&mut env);
        format!("the SDK isn't in this build ({e})")
    })?;
    AVAILABLE.store(true, Ordering::Relaxed);

    let mut waiting: Vec<Achievement> = Vec::new();
    play.refresh(&mut env, false, &mut waiting);
    while let Ok(command) = commands.recv() {
        match command {
            Command::Report(list) => {
                waiting.extend(list);
                play.flush(&mut env, &mut waiting);
            }
            Command::SignIn => play.refresh(&mut env, true, &mut waiting),
        }
    }
    Ok(())
}

struct Play<'a> {
    activity: &'a JObject<'a>,
    /// `com.google.android.gms.games.PlayGames`
    games: JClass<'a>,
    /// `com.google.android.gms.tasks.Tasks`
    tasks: JClass<'a>,
}

impl<'a> Play<'a> {
    fn new(env: &mut JNIEnv<'a>, activity: &'a JObject<'a>) -> jni::errors::Result<Play<'a>> {
        let loader = env.call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
        let mut load = |name: &str| -> jni::errors::Result<JClass<'a>> {
            let name = env.new_string(name)?;
            let class = env
                .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?
                .l()?;
            Ok(JClass::from(class))
        };
        let sdk = load("com.google.android.gms.games.PlayGamesSdk")?;
        let games = load("com.google.android.gms.games.PlayGames")?;
        let tasks = load("com.google.android.gms.tasks.Tasks")?;
        env.call_static_method(&sdk, "initialize", "(Landroid/content/Context;)V", &[JValue::Object(activity)])?;
        Ok(Play { activity, games, tasks })
    }

    /// Finds out whether the player is signed in (asking them to, if `ask`), and
    /// if they've just arrived, reads their trophies and sends what's waiting.
    fn refresh(&self, env: &mut JNIEnv<'a>, ask: bool, waiting: &mut Vec<Achievement>) {
        let signed_in = match self.check_sign_in(env, ask) {
            Ok(signed_in) => signed_in,
            Err(e) => {
                clear_exception(env);
                crate::assets::log_warn(&format!("Play Games sign-in failed: {e}"));
                false
            }
        };
        let was = SIGNED_IN.swap(signed_in, Ordering::Relaxed);
        if signed_in
            && !was
            && let Err(e) = self.load(env)
        {
            clear_exception(env);
            crate::assets::log_warn(&format!("couldn't read Play Games trophies: {e}"));
        }
        self.flush(env, waiting);
    }

    fn check_sign_in(&self, env: &mut JNIEnv<'a>, ask: bool) -> jni::errors::Result<bool> {
        let client = env
            .call_static_method(
                &self.games,
                "getGamesSignInClient",
                "(Landroid/app/Activity;)Lcom/google/android/gms/games/GamesSignInClient;",
                &[JValue::Object(self.activity)],
            )?
            .l()?;
        let method = if ask { "signIn" } else { "isAuthenticated" };
        let task = env.call_method(&client, method, "()Lcom/google/android/gms/tasks/Task;", &[])?.l()?;
        let result = self.wait(env, &task)?;
        env.call_method(&result, "isAuthenticated", "()Z", &[])?.z()
    }

    /// `Tasks.await(task)`
    fn wait(&self, env: &mut JNIEnv<'a>, task: &JObject) -> jni::errors::Result<JObject<'a>> {
        env.call_static_method(
            &self.tasks,
            "await",
            "(Lcom/google/android/gms/tasks/Task;)Ljava/lang/Object;",
            &[JValue::Object(task)],
        )?
        .l()
    }

    fn client(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<JObject<'a>> {
        env.call_static_method(
            &self.games,
            "getAchievementsClient",
            "(Landroid/app/Activity;)Lcom/google/android/gms/games/AchievementsClient;",
            &[JValue::Object(self.activity)],
        )?
        .l()
    }

    /// Sends what's waiting, if the player is signed in. `unlock` needs no answer:
    /// the SDK queues it while offline, and showing its own popup is fine by Google.
    fn flush(&self, env: &mut JNIEnv<'a>, waiting: &mut Vec<Achievement>) {
        if !signed_in() {
            return;
        }
        let list = std::mem::take(waiting);
        let sent: jni::errors::Result<()> = (|| {
            let client = self.client(env)?;
            for id in list.iter().filter_map(|&a| play_id(a)) {
                let id = env.new_string(id)?;
                env.call_method(&client, "unlock", "(Ljava/lang/String;)V", &[JValue::Object(&id)])?;
                env.delete_local_ref(id)?;
            }
            env.delete_local_ref(client)
        })();
        // Everything goes up again at the next launch, so this heals by itself.
        if let Err(e) = sent {
            clear_exception(env);
            crate::assets::log_warn(&format!("Play Games didn't take a trophy: {e}"));
        }
    }

    /// Asks what the player has already earned, on any device.
    fn load(&self, env: &mut JNIEnv<'a>) -> jni::errors::Result<()> {
        let client = self.client(env)?;
        let task = env
            .call_method(&client, "load", "(Z)Lcom/google/android/gms/tasks/Task;", &[JValue::Bool(0)])?
            .l()?;
        let annotated = self.wait(env, &task)?;
        let buffer = env.call_method(&annotated, "get", "()Ljava/lang/Object;", &[])?.l()?;
        let count = env.call_method(&buffer, "getCount", "()I", &[])?.i()?;
        for i in 0..count {
            let item = env.call_method(&buffer, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])?.l()?;
            let state = env.call_method(&item, "getState", "()I", &[])?.i()?;
            if state == STATE_UNLOCKED {
                let id = env.call_method(&item, "getAchievementId", "()Ljava/lang/String;", &[])?.l()?;
                let id: String = env.get_string(&JString::from(id))?.into();
                if let Some(a) = from_play_id(&id) {
                    super::store_has(a);
                }
            }
            env.delete_local_ref(item)?;
        }
        env.call_method(&buffer, "release", "()V", &[])?;
        Ok(())
    }
}

fn clear_exception(env: &mut JNIEnv) {
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
}
