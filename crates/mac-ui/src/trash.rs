//! Moving a file or a folder to the Trash (`NSFileManager trashItemAtURL:resultingItemURL:error:`)
//! instead of deleting it, so the user can still get it back from the Finder. No unsafe: the
//! binding is safe in objc2-foundation.

use std::path::Path;

use objc2_foundation::{NSFileManager, NSString, NSURL};

/// Move `path` (a file, or a folder with everything in it) to the Trash of its volume. `false`
/// when that cannot be done (a volume without a Trash, no access, the path is gone or not
/// UTF-8): it is then left where it was, untouched. Safe on any thread (the default file
/// manager is thread-safe). Nothing is logged.
pub fn move_to_trash(path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    objc2::rc::autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .is_ok()
    })
}
