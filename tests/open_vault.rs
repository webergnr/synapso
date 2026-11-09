//! ⌘O: the folder you pick becomes the vault, and is still the vault next
//! launch.
//!
//! ⚠️ its own integration binary on purpose. `vault::set_root` writes a
//! process-wide choice and this test sets `SYNAPSO_VAULT` and
//! `SYNAPSO_CONFIG`; run beside the unit tests it would hand every other test
//! in that binary the vault this one picked.
//!
//! ⚠️ what is NOT proved here: the dialog. `rfd::FileDialog::pick_folder`
//! opens a native `NSOpenPanel` and spins its own modal loop, which no script
//! can answer — so ⌘O is proved in two halves, the panel by hand and
//! everything downstream of the path it returns by this.

use std::fs;
use std::path::Path;

use synapso_core::vault;

fn names(root: &Path) -> Vec<String> {
    vault::list(root, "md")
        .iter()
        .map(|p| vault::name(p))
        .collect()
}

#[test]
fn a_picked_folder_becomes_the_vault_and_is_remembered() {
    let base = std::env::temp_dir().join(format!("synapso-openvault-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (first, second) = (base.join("first"), base.join("second"));
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("one.md"), "# one\n").unwrap();
    fs::write(second.join("two.md"), "# two\n").unwrap();

    // SAFETY: this test owns its process — see the module note.
    unsafe {
        std::env::set_var("SYNAPSO_CONFIG", base.join("config"));
        std::env::set_var("SYNAPSO_VAULT", &first);
    }

    assert_eq!(vault::root(), first, "the env var is where a launch starts");
    assert_eq!(names(&vault::root()), ["one"]);
    assert!(
        vault::remembered().is_none(),
        "nothing has been picked yet, so there is nothing to remember"
    );

    let landed = vault::set_root(&second).unwrap();
    assert_eq!(landed, second);
    // ⚠️ the pick OUTRANKS the env var. Weber runs the app with
    // `SYNAPSO_VAULT` set; if the variable won, ⌘O would do nothing in the one
    // setup it is most often tried in.
    assert_eq!(vault::root(), second);
    assert_eq!(names(&vault::root()), ["two"]);
    assert_eq!(
        vault::remembered(),
        Some(second.clone()),
        "and the next launch, with no env var, opens it again"
    );

    // a folder that is not there yet is created, not refused — the same rule
    // `vault::root()` has always had for `~/Synapso`
    let third = base.join("third");
    assert_eq!(vault::set_root(&third).unwrap(), third);
    assert!(third.is_dir());
    assert!(names(&vault::root()).is_empty());

    let _ = fs::remove_dir_all(&base);
}
