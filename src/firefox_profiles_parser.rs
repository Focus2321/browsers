use std::collections::HashMap;
use std::path::Path;

use configparser::ini::{Ini, IniDefault};
use naive_cityhash::cityhash64;
use tracing::{debug, info, warn};

use crate::{InstalledBrowserProfile, utils};

pub fn find_firefox_profiles(
    firefox_profiles_dir: &Path,
    binary_path: &Path,
    _app_id: &str,
) -> Vec<InstalledBrowserProfile> {
    let binary_dir = binary_path.parent().and_then(|p| p.to_str()).unwrap_or("");
    // binary_dir is the path where binary is (without trailing slash)
    let install_dir_hash = hash_firefox_install_dir(binary_dir);
    //run_arg = r#"open -b com.google.Chrome -n --args --profile-directory="Profile 1" https://www.google.com"#

    info!("Dir {}", binary_path.to_str().unwrap());
    info!("Dir `{}` has hash {}", binary_dir, install_dir_hash.as_str());

    let mut browser_profiles: Vec<InstalledBrowserProfile> = Vec::new();

    let profiles_ini_path = firefox_profiles_dir.join("profiles.ini");
    debug!("profiles_ini_path: {:?}", profiles_ini_path);

    // When user has installed Firefox, but never ran it, then the profiles.ini does not yet exist
    if !profiles_ini_path.exists() {
        info!(
            "Skipping Firefox, because profiles.ini does not exist at '{}'",
            profiles_ini_path.display()
        );

        return browser_profiles;
    }

    let mut ini_default = IniDefault::default();
    ini_default.case_sensitive = true;
    let mut profiles_ini_config = Ini::new_from_defaults(ini_default);

    // You can easily load a file to get a clone of the map.
    // Unreadable (e.g denied by macOS App Data protection) means "no profiles", so Firefox still
    // shows up with its placeholder profile.
    let profiles_ini_map = match profiles_ini_config.load(&profiles_ini_path) {
        Ok(profiles_ini_map) => profiles_ini_map,
        Err(err) => {
            warn!("Could not read {}: {}", profiles_ini_path.display(), err);
            return browser_profiles;
        }
    };
    //info!("{:?}", map);
    // You can also safely not store the reference and access it later with get_map_ref() or get a clone with get_map()

    // Make two runs over profiles.ini
    // if profile has a hash, but it's not our hash then skip it (except when disabling hash feature?)

    // profile locked to an firefox instance
    let mut locked_profile_path_and_hash: HashMap<String, String> = HashMap::new();

    // 1. find all default profiles,
    //    see https://support.mozilla.org/en-US/kb/understanding-depth-profile-installation
    for (install_key, install_values) in profiles_ini_map.into_iter() {
        // e.g [Install9F3C89D8F8FDBC89]
        if !install_key.starts_with("Install") {
            continue;
        }
        // e.g "9F3C89D8F8FDBC89"
        let installation_dir_hashed = install_key.strip_prefix("Install").unwrap();
        if !install_values.contains_key("Default") {
            continue;
        }
        if !install_values.contains_key("Locked") {
            continue;
        }
        // build a map of profile to installation hash?

        /*if installation_dir_hashed != install_dir_hash {
            // skip if not a rule for this browser
            continue;
        }*/

        // can be relative or absolute
        let default_profile_path = install_values
            .get("Default")
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .to_string();

        let default_locked = install_values
            .get("Locked")
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .to_string();
        let is_profile_locked = default_locked == "1";

        if is_profile_locked {
            locked_profile_path_and_hash
                .insert(default_profile_path.clone(), installation_dir_hashed.to_string());
        }
    }

    let profiles_ini_map = match profiles_ini_config.load(&profiles_ini_path) {
        Ok(profiles_ini_map) => profiles_ini_map,
        Err(err) => {
            warn!("Could not read {}: {}", profiles_ini_path.display(), err);
            return browser_profiles;
        }
    };
    for (_profile_key, profile_values) in profiles_ini_map.into_iter() {
        if !profile_values.contains_key("Path") {
            continue;
        }
        // can be relative or absolute
        let profile_path = profile_values
            .get("Path")
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .to_string();

        let profile_hash_maybe = locked_profile_path_and_hash.get(profile_path.as_str());
        if let Some(profile_hash) = profile_hash_maybe {
            // if this profile has some other hash than current firefox, then skip the profile
            if profile_hash.to_string() != install_dir_hash {
                // `install_dir_hash`, that we calculate based on binary path,
                // is not always correct, e.g in case of snap package in linux,
                // so, let's not use it yet to exclude profiles.
                // Users can always manually hide these profiles instead.
                // TODO: https://github.com/Browsers-software/barowsers/issues/81
                //continue;
            }
        }

        let profile_dir = if !profile_path.starts_with("/") {
            firefox_profiles_dir.join(profile_path.as_str())
        } else {
            Path::new(profile_path.as_str()).to_path_buf()
        };
        if !profile_dir.exists() {
            info!(
                "Skipping profile directory '{}', because it does not exist",
                profile_dir.display()
            );
            continue;
        }

        let mut containers = Vec::new();

        let mut open_url_in_container_extension_installed = false;
        let extensions_json_file = profile_dir.join("extensions.json");
        if !extensions_json_file.exists() {
            info!(
                "Skipping containers for profile '{}', because it does not have extensions.json file",
                profile_dir.display()
            );
        } else {
            open_url_in_container_extension_installed =
                has_open_url_in_container_extension_installed(extensions_json_file.as_path());
        }

        if open_url_in_container_extension_installed {
            let containers_json_file = profile_dir.join("containers.json");
            if !containers_json_file.exists() {
                info!(
                    "Skipping containers for profile '{}', because it does not have containers.json file",
                    profile_dir.display()
                );
            } else {
                // containers for this profile
                info!(
                    "Checking containers from {}",
                    containers_json_file.as_path().to_str().unwrap()
                );
                containers = containers_json_map(containers_json_file.as_path());
            }
        }

        let name_maybe = profile_values
            .get("Name")
            .and_then(|a| a.as_ref())
            .map(|b| b.to_string());

        let Some(profile_name) = name_maybe else {
            continue;
        };

        // Even if profile has containers, also add a non-container option
        browser_profiles.push(InstalledBrowserProfile {
            profile_cli_arg_value: profile_name.to_string(),
            profile_cli_container_name: None,
            profile_name: profile_name.to_string(),
            profile_icon: None,
            profile_restricted_url_patterns: vec![],
        });

        if !containers.is_empty() {
            for container in containers {
                browser_profiles.push(InstalledBrowserProfile {
                    profile_cli_arg_value: profile_name.to_string(),
                    profile_cli_container_name: Some(container.id.to_string()),
                    profile_name: profile_name.to_string() + " " + container.name.as_str(),
                    profile_icon: None,
                    profile_restricted_url_patterns: vec![],
                })
            }
        }
    }

    return browser_profiles;
}

// has "open-url-in-container" extension installed, which adds "ext+container" protocol support
fn has_open_url_in_container_extension_installed(extensions_json_file_path: &Path) -> bool {
    let extension_id = "{f069aec0-43c5-4bbf-b6b4-df95c4326b98}";
    let extension2_id = "openincontainer@browsers.software";

    let Some(v) = utils::read_json_file(extensions_json_file_path) else {
        return false;
    };
    let Some(addons_arr) = v["addons"].as_array() else {
        return false;
    };
    for addon in addons_arr {
        let addon_id = addon["id"].as_str().unwrap_or("");
        // https://addons.mozilla.org/en-US/firefox/addon/open-url-in-container/

        if addon_id == extension_id || addon_id == extension2_id {
            let is_active = addon["active"].as_bool().unwrap_or(false);
            return is_active;
        }
    }

    return false;
}

fn containers_json_map(containers_json_file_path: &Path) -> Vec<FirefoxContainer> {
    let mut containers: Vec<FirefoxContainer> = Vec::new();

    let Some(v) = utils::read_json_file(containers_json_file_path) else {
        return containers;
    };
    let Some(identities_arr) = v["identities"].as_array() else {
        return containers;
    };

    for identity in identities_arr {
        let is_public = identity["public"].as_bool().unwrap_or(false);
        if is_public {
            let name = if let Some(l10n_id) = identity["l10nID"].as_str() {
                // old ones
                match l10n_id {
                    "userContextPersonal.label" => "Personal",
                    "userContextWork.label" => "Work",
                    "userContextBanking.label" => "Banking",
                    "userContextShopping.label" => "Shopping",
                    _ => "Unknown",
                }
            } else if let Some(l10n_id) = identity["l10nId"].as_str() {
                // new ones
                // https://github.com/mozilla/gecko-dev/commit/94b1ab321c1d5c4552716a7f3e7446f0bee28db9
                match l10n_id {
                    "user-context-personal" => "Personal",
                    "user-context-work" => "Work",
                    "user-context-banking" => "Banking",
                    "user-context-shopping" => "Shopping",
                    _ => "Unknown",
                }
            } else {
                identity["name"].as_str().unwrap_or("Unknown")
            };

            let container = FirefoxContainer {
                id: name.to_string(),
                name: name.to_string(),
            };
            containers.push(container);
        }
    }
    return containers;
}

struct FirefoxContainer {
    id: String,
    // aka cookie store id in Firefox
    name: String,
}

fn hash_firefox_install_dir(ff_binary_dir: &str) -> String {
    // ff_binary_dir has to be absolute and not contain trailing slash
    // see https://github.com/mozilla/gecko-dev/blob/d36cf98aa85f24ceefd07521b3d16b9edd2abcb7/toolkit/mozapps/update/common/commonupdatedir.cpp#L761
    // e.g "/Applications/Firefox.app/Contents/MacOS" as that is where "firefox" binary lives

    let path_as_utf16_bytes: Vec<u8> = ff_binary_dir
        .encode_utf16()
        .flat_map(|a| a.to_le_bytes())
        .collect();

    let path_as_utf16_slice = path_as_utf16_bytes.as_slice();

    let hash_u64: u64 = cityhash64(path_as_utf16_slice);
    let hash_u64_str = format!("{:X}", hash_u64);
    return hash_u64_str;
}
