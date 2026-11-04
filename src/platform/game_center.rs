//! Game Center on iOS, through GameKit.
//!
//! Signing in is GameKit's: it calls the authenticate handler when it knows
//! whether the player is signed in, or with a sign-in screen to show. Once
//! signed in, the trophies the player has elsewhere come back to the game and
//! everything earned here goes up. GameKit queues reports made while offline.
//!
//! The app needs the Game Center capability on its App ID, which puts the
//! entitlement in the provisioning profile `cargo love2drs` signs with. Without
//! it (the simulator's ad hoc signature, say) the handler reports an error and
//! the game carries on with local trophies.
//!
//! The game's banner shows new trophies, so GameKit's is turned off.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
use objc2::{class, ffi, msg_send, sel};
use objc2_foundation::{NSArray, NSError, NSString};

use crate::euchre::record::Achievement;

#[link(name = "GameKit", kind = "framework")]
unsafe extern "C" {}

/// A Game Center id is this plus the trophy's id.
const ID_PREFIX: &str = "com.gccody.cabineuchre.";

static SIGNED_IN: AtomicBool = AtomicBool::new(false);
/// Earned and waiting for the player to be signed in.
static WAITING: Mutex<Vec<Achievement>> = Mutex::new(Vec::new());
/// GameKit's sign-in screen, until the main thread can show it.
static SIGN_IN_SCREEN: Mutex<Option<Handed>> = Mutex::new(None);

/// Sends a message `GKAchievement` forwards to an object behind it. Its properties
/// are real, but the class has no such methods, so the check `msg_send!` makes in
/// debug builds would abort on them. Only for pointers, numbers and `Bool`: `Arg`
/// and `R` have to be the method's real types.
unsafe fn forwarded<Arg, R>(receiver: &AnyObject, selector: Sel, arg: Arg) -> R {
    // SAFETY: objc_msgSend takes the receiver and selector, then the method's own
    // arguments, so it's called with the method's signature.
    unsafe {
        let send: unsafe extern "C" fn(&AnyObject, Sel, Arg) -> R = std::mem::transmute(ffi::objc_msgSend as *const ());
        send(receiver, selector, arg)
    }
}

/// [`forwarded`] for a getter.
unsafe fn forwarded_get<R>(receiver: &AnyObject, selector: Sel) -> R {
    // SAFETY: As in `forwarded`.
    unsafe {
        let send: unsafe extern "C" fn(&AnyObject, Sel) -> R = std::mem::transmute(ffi::objc_msgSend as *const ());
        send(receiver, selector)
    }
}

/// A view controller GameKit handed over, from whichever thread it called on.
/// It's only touched again on the main thread.
struct Handed(Retained<AnyObject>);
// SAFETY: Retaining and releasing are thread-safe, and only `update`, which runs
// on the main thread, ever sends this a message.
unsafe impl Send for Handed {}

/// Runs on the main thread, once. GameKit keeps the handler and calls it again
/// whenever the player's state changes, like coming back from Settings.
pub fn init() {
    let handler = RcBlock::new(|screen: *mut AnyObject, error: *mut NSError| {
        // SAFETY: GameKit passes a view controller or nil, and an NSError or nil.
        if let Some(screen) = unsafe { Retained::retain(screen) }
            && let Ok(mut slot) = SIGN_IN_SCREEN.lock()
        {
            *slot = Some(Handed(screen));
            return;
        }
        if let Some(error) = unsafe { error.as_ref() } {
            crate::assets::log_warn(&format!("Game Center is off: {}", error.localizedDescription()));
        }
        signed_in_changed();
    });
    // SAFETY: These are GameKit's documented signatures (iOS 14 and up), and the
    // setter copies the block.
    unsafe {
        let player: Retained<AnyObject> = msg_send![class!(GKLocalPlayer), local];
        let _: () = msg_send![&player, setAuthenticateHandler: &*handler];
    }
}

fn signed_in_changed() {
    // SAFETY: `local` and `isAuthenticated` take no arguments and are safe on any thread.
    let signed_in: bool = unsafe {
        let player: Retained<AnyObject> = msg_send![class!(GKLocalPlayer), local];
        msg_send![&player, isAuthenticated]
    };
    let was = SIGNED_IN.swap(signed_in, Ordering::Relaxed);
    if signed_in && !was {
        load_from_store();
        let waiting = WAITING.lock().map(|mut w| std::mem::take(&mut *w)).unwrap_or_default();
        send(&waiting);
    }
}

/// Shows GameKit's sign-in screen once there's something to show it over. Call every frame.
pub fn update() {
    let Ok(mut slot) = SIGN_IN_SCREEN.lock() else { return };
    if slot.is_none() {
        return;
    }
    // SAFETY: `update` runs on the main thread, and these are UIKit's documented
    // signatures. A window scene has `keyWindow` from iOS 15, the deployment target.
    unsafe {
        let Some(root) = root_controller() else { return };
        let Some(Handed(screen)) = slot.take() else { return };
        let no_completion: Option<&block2::Block<dyn Fn()>> = None;
        let _: () = msg_send![&root, presentViewController: &*screen, animated: true, completion: no_completion];
    }
}

/// The view controller at the root of the key window.
unsafe fn root_controller() -> Option<Retained<AnyObject>> {
    unsafe {
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let scenes: Retained<AnyObject> = msg_send![&app, connectedScenes];
        let scenes: Retained<NSArray<AnyObject>> = msg_send![&scenes, allObjects];
        let window_scene: &AnyClass = class!(UIWindowScene);
        for scene in scenes.iter() {
            let is_window_scene: bool = msg_send![&scene, isKindOfClass: window_scene];
            if !is_window_scene {
                continue;
            }
            let window: Option<Retained<AnyObject>> = msg_send![&scene, keyWindow];
            let root: Option<Retained<AnyObject>> = match window {
                Some(window) => msg_send![&window, rootViewController],
                None => None,
            };
            if root.is_some() {
                return root;
            }
        }
        None
    }
}

pub fn signed_in() -> bool {
    SIGNED_IN.load(Ordering::Relaxed)
}

/// Sends these trophies now, or as soon as the player is signed in.
pub fn report(list: &[Achievement]) {
    if signed_in() {
        send(list);
    } else if let Ok(mut waiting) = WAITING.lock() {
        waiting.extend(list);
    }
}

fn send(list: &[Achievement]) {
    if list.is_empty() {
        return;
    }
    // SAFETY: GKAchievement's documented signatures; the block is copied by the call.
    unsafe {
        let reports: Vec<Retained<AnyObject>> = list
            .iter()
            .map(|a| {
                let id = NSString::from_str(&format!("{ID_PREFIX}{}", a.id()));
                let report: Allocated<AnyObject> = msg_send![class!(GKAchievement), alloc];
                let report: Retained<AnyObject> = msg_send![report, initWithIdentifier: &*id];
                forwarded::<f64, ()>(&report, sel!(setPercentComplete:), 100.0);
                forwarded::<Bool, ()>(&report, sel!(setShowsCompletionBanner:), Bool::NO);
                report
            })
            .collect();
        let reports = NSArray::from_retained_slice(&reports);
        let done = RcBlock::new(|error: *mut NSError| {
            // Everything goes up again at the next launch, so this heals by itself.
            if let Some(error) = error.as_ref() {
                crate::assets::log_warn(&format!("Game Center didn't take a trophy: {}", error.localizedDescription()));
            }
        });
        let _: () = msg_send![class!(GKAchievement), reportAchievements: &*reports, withCompletionHandler: &*done];
    }
}

/// Asks what the player has already earned, on any device.
fn load_from_store() {
    let done = RcBlock::new(|list: *mut NSArray<AnyObject>, error: *mut NSError| {
        // SAFETY: GameKit passes an array of GKAchievement or nil, and an NSError or nil.
        unsafe {
            if let Some(error) = error.as_ref() {
                crate::assets::log_warn(&format!("couldn't read Game Center trophies: {}", error.localizedDescription()));
            }
            let Some(list) = list.as_ref() else { return };
            for item in list.iter() {
                let completed = forwarded_get::<Bool>(&item, sel!(isCompleted));
                let id = forwarded_get::<*mut NSString>(&item, sel!(identifier));
                if completed.as_bool()
                    && let Some(id) = id.as_ref()
                    && let Some(a) = id.to_string().strip_prefix(ID_PREFIX).and_then(Achievement::from_id)
                {
                    super::store_has(a);
                }
            }
        }
    });
    // SAFETY: GKAchievement's documented signature; the block is copied by the call.
    unsafe {
        let _: () = msg_send![class!(GKAchievement), loadAchievementsWithCompletionHandler: &*done];
    }
}
