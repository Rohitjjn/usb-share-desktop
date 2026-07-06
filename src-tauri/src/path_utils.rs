use std::path::{Path, PathBuf};

/// Resolves a requested relative path against the shared root path.
/// It canonicalizes the resulting path and verifies that it is indeed
/// either the root path itself or a child of the root path.
/// This prevents sibling-folder-name traversal bugs (e.g. root "C:\Share" and req "..\ShareBackup")
/// and ".." path traversal.
pub fn resolve_and_verify_path(root_path: &Path, request_path: &str) -> Option<PathBuf> {
    // Basic sanitization
    let clean_request = request_path.trim_start_matches('/').replace("\\", "/");
    let clean_request = clean_request.trim_start_matches('/');

    // Join with root
    let full_path = root_path.join(clean_request);
    
    // Canonicalize both
    let canonical_root = root_path.canonicalize().ok()?;
    let canonical_full = full_path.canonicalize().ok()?;

    // Check if canonical_full starts with canonical_root
    // We must be careful to avoid sibling folder matches when using strings.
    // E.g. "C:\Share" and "C:\ShareBackup"
    // Using Path::starts_with handles this correctly by comparing path components.
    if canonical_full.starts_with(&canonical_root) {
        Some(canonical_full)
    } else {
        None
    }
}

/// Same as resolve_and_verify_path, but doesn't require the target to exist.
/// Helpful for MKCOL, PUT, etc. where we are creating a new file/folder.
pub fn resolve_and_verify_path_for_creation(root_path: &Path, request_path: &str) -> Option<PathBuf> {
    let clean_request = request_path.trim_start_matches('/').replace("\\", "/");
    let clean_request = clean_request.trim_start_matches('/');

    let full_path = root_path.join(clean_request);
    
    let canonical_root = root_path.canonicalize().ok()?;
    
    // Since the target might not exist, we can't canonicalize it directly.
    // Instead, canonicalize its parent, verify the parent, and then join the file name.
    if let Some(parent) = full_path.parent() {
        if let Ok(canonical_parent) = parent.canonicalize() {
            if canonical_parent.starts_with(&canonical_root) {
                if let Some(file_name) = full_path.file_name() {
                    return Some(canonical_parent.join(file_name));
                } else {
                    return Some(canonical_parent); // It was just the root
                }
            }
        }
    }
    
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::env;

    #[test]
    fn test_path_safety() {
        // Create a temporary directory structure
        let temp_dir = env::temp_dir().join("usb_share_test_path_safety");
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        let share_dir = temp_dir.join("Share");
        let sibling_dir = temp_dir.join("ShareBackup");
        
        fs::create_dir(&share_dir).unwrap();
        fs::create_dir(&sibling_dir).unwrap();
        
        let valid_file = share_dir.join("valid.txt");
        fs::write(&valid_file, "hello").unwrap();
        
        let sibling_file = sibling_dir.join("secret.txt");
        fs::write(&sibling_file, "secret").unwrap();

        // 1. Valid child access
        let resolved = resolve_and_verify_path(&share_dir, "valid.txt").unwrap();
        assert_eq!(resolved.canonicalize().unwrap(), valid_file.canonicalize().unwrap());

        // 2. Directory traversal attempt
        let traversal = resolve_and_verify_path(&share_dir, "../ShareBackup/secret.txt");
        assert!(traversal.is_none());

        // 3. Absolute path attempt
        let absolute_req = sibling_file.to_string_lossy().to_string();
        let traversal2 = resolve_and_verify_path(&share_dir, &absolute_req);
        // Depending on Path::join behavior with absolute paths on different OS, 
        // it might replace the base. The starts_with check will catch it.
        assert!(traversal2.is_none());

        // 4. Creation valid
        let creation_valid = resolve_and_verify_path_for_creation(&share_dir, "new_folder/new_file.txt");
        // new_folder doesn't exist yet, so canonicalize of parent will fail. 
        // We should test creation in an existing parent.
        let creation_valid_existing_parent = resolve_and_verify_path_for_creation(&share_dir, "new_file.txt").unwrap();
        assert_eq!(creation_valid_existing_parent, share_dir.canonicalize().unwrap().join("new_file.txt"));
        
        // 5. Creation traversal attempt
        let creation_traversal = resolve_and_verify_path_for_creation(&share_dir, "../ShareBackup/new_file.txt");
        assert!(creation_traversal.is_none());
        
        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
