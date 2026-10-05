//! # Module contract
//!
//! ```json
//! {"purpose":"Say where a project's grimoire lives: resolve a store argument or a project file's entry the one way every command uses, find and write a repository's .grimoire.toml, which names a store or a team's server, say where a checkout's copy of a server's intent and a person's tokens are kept, and refuse a store inside the repository it describes.","criteria":[{"id":"resolution","statement":"A store lives under <data>/grimoire/stores/<name> unless given as a path, <data> being GRIMOIRE_HOME when it is set and the platform's data directory otherwise; a store argument or a repository's .grimoire.toml is resolved one way, an absolute path as it is, a bare name of one plain component under the stores directory, and anything else, . and .. included, as a path relative to the repository."},{"id":"outside-repository","statement":"refuse_inside refuses a store that resolves inside the repository it describes, through . and .. or links, whether or not the store exists yet, and accepts one beside or above it."},{"id":"project-name","statement":"A project file's grimoire is named by its `name`, which must be a slug or the file is refused; with none, by its store when that is a bare name, and otherwise by its repository folder's name, each made a slug; init writes the name it is given or that folder name, before the store or server."},{"id":"store-or-server","statement":"A .grimoire.toml names `store` or `server` and never both: one naming both or neither is refused as invalid, as is a server that is not an http:// or https:// address with a host; a server is kept without its trailing slash, and its project's store is this checkout's copy of the server's intent, under <data>/grimoire/copies, in a folder named for the server and the checkout, so two checkouts or two servers never share one; a person's tokens are kept in credentials.json, under GRIMOIRE_HOME when it is set and the platform's configuration directory otherwise. A server's address carries no query or fragment, and names a grimoire of a team's library only last, as /g/<slug>; server_grimoire reads that name from it."}]}
//! ```

use std::path::{Path, PathBuf};

use super::StoreIoError;

pub const PROJECT_FILE: &str = ".grimoire.toml";

/// When set, the directory Grimoire keeps its stores, its copies and a
/// person's tokens under, in place of the platform's data and configuration
/// directories: for a pipeline, a container, or a test.
pub const HOME_ENV: &str = "GRIMOIRE_HOME";

fn home() -> Option<PathBuf> {
    std::env::var_os(HOME_ENV).filter(|h| !h.is_empty()).map(PathBuf::from)
}

fn platform() -> Result<directories::ProjectDirs, StoreIoError> {
    directories::ProjectDirs::from("", "", "grimoire").ok_or(StoreIoError::NoDataDir)
}

fn data_dir() -> Result<PathBuf, StoreIoError> {
    match home() {
        Some(home) => Ok(home),
        None => Ok(platform()?.data_dir().to_path_buf()),
    }
}

/// The directory for stores: `<data>/grimoire/stores`.
pub fn stores_dir() -> Result<PathBuf, StoreIoError> {
    Ok(data_dir()?.join("stores"))
}

/// The directory for checkouts' copies of a server's intent: `<data>/grimoire/copies`.
pub fn copies_dir() -> Result<PathBuf, StoreIoError> {
    Ok(data_dir()?.join("copies"))
}

/// The file a person's tokens are kept in, outside any repository.
pub fn credentials_file() -> Result<PathBuf, StoreIoError> {
    match home() {
        Some(home) => Ok(home.join("credentials.json")),
        None => Ok(platform()?.config_dir().join("credentials.json")),
    }
}

/// What a project's file names as the home of its grimoire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Home {
    /// A store on this machine, resolved.
    Store(PathBuf),
    /// A team's server, by address, and where this checkout's copy of its intent is kept.
    Server { address: String, copy: PathBuf },
}

impl Home {
    /// The store every command reads: the project's own, or its copy of the server's.
    pub fn dir(&self) -> &Path {
        match self {
            Home::Store(dir) | Home::Server { copy: dir, .. } => dir,
        }
    }
}

/// A server's address as it is kept: `http://` or `https://` and a host,
/// with no trailing slash. Anything else is refused, with why.
pub fn server_address(given: &str) -> Result<String, String> {
    let address = given.trim();
    let (scheme, rest) =
        address.split_once("://").filter(|(scheme, _)| matches!(*scheme, "http" | "https")).ok_or_else(|| format!("`{given}` is not a server's address: it starts with http:// or https://"))?;
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() || rest.starts_with('/') || rest.chars().any(char::is_whitespace) {
        return Err(format!("`{given}` is not a server's address: it names no host"));
    }
    if rest.contains(['?', '#']) {
        return Err(format!("`{given}` is not a server's address: it carries a query or a fragment"));
    }
    // A grimoire of a team's library is named last, by a slug: `/g/<name>`, after any prefix a proxy adds.
    let segments: Vec<&str> = rest.split('/').skip(1).collect();
    if let Some(at) = segments.iter().position(|s| *s == "g") {
        if at + 2 != segments.len() || !grimoire_core::store::is_slug(segments[at + 1]) {
            return Err(format!("`{given}` is not a server's address: a grimoire of a team's library is named last, as /g/<name>, the name a slug"));
        }
    }
    Ok(format!("{scheme}://{rest}"))
}

/// The grimoire a server's address names in its team's library, `/g/<name>` at
/// its end; none for a server of one grimoire.
pub fn server_grimoire(address: &str) -> Option<String> {
    let (_, rest) = address.split_once("://")?;
    let mut segments = rest.trim_end_matches('/').rsplit('/');
    let (name, g) = (segments.next()?, segments.next()?);
    (g == "g" && grimoire_core::store::is_slug(name)).then(|| name.to_string())
}

/// A canonical path as a store records it: `/`-separated, without the
/// `\\?\` prefix Windows canonical paths carry, and a share, which comes back
/// as `\\?\UNC\server\share`, as `//server/share`.
pub fn slashed(canonical: &str) -> String {
    let s = match canonical.strip_prefix(r"\\?\UNC\") {
        Some(share) => format!(r"\\{share}"),
        None => canonical.strip_prefix(r"\\?\").unwrap_or(canonical).to_string(),
    };
    s.replace('\\', "/")
}

/// The repository at `repo` as a store records it: its canonical path, slashed.
pub fn repository_string(repo: &Path) -> Result<String, StoreIoError> {
    let canonical = repo.canonicalize().map_err(|e| StoreIoError::Io(repo.to_path_buf(), e))?;
    Ok(slashed(&canonical.to_string_lossy()))
}

/// Where the checkout at `repo` keeps its copy of the intent of the server
/// at `address`: a folder named for both, so no two share one.
pub fn copy_dir(address: &str, repo: &Path) -> Result<PathBuf, StoreIoError> {
    use sha2::{Digest, Sha256};
    let checkout = repository_string(repo)?;
    let digest: String = Sha256::digest(format!("{address}\n{checkout}").as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect();
    let host: String = address.split_once("://").map_or(address, |(_, rest)| rest).chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).take(40).collect();
    Ok(copies_dir()?.join(format!("{}-{digest}", host.trim_matches('-'))))
}

/// Walk up from `start` to the nearest `.grimoire.toml` and return the
/// directory holding it and what it names: a store, resolved, or a server
/// with this checkout's copy of its intent.
pub fn find_home(start: &Path) -> Result<(PathBuf, Home), StoreIoError> {
    find_named(start).map(|(dir, home, _)| (dir, home))
}

/// The same, with the grimoire's name (req:project-name).
pub fn find_named(start: &Path) -> Result<(PathBuf, Home, String), StoreIoError> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if let Some((home, name)) = project_at(&d)? {
            return Ok((d, home, name));
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Err(StoreIoError::NoProjectFile(start.to_path_buf()))
}

/// What the `.grimoire.toml` in `d` itself names, and its grimoire's name;
/// none when `d` holds no project file. The name is the file's `name`, else
/// the store's when that is a bare name, else the folder's, each a slug.
pub fn project_at(d: &Path) -> Result<Option<(Home, String)>, StoreIoError> {
    let f = d.join(PROJECT_FILE);
    if !f.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&f).map_err(|e| StoreIoError::Io(f.clone(), e))?;
    let doc: toml::Table = text.parse().map_err(|e: toml::de::Error| StoreIoError::BadProjectFile(f.clone(), e.to_string()))?;
    let bad = |why: String| StoreIoError::BadProjectFile(f.clone(), why);
    let named = |key: &str| match doc.get(key) {
        None => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| bad(format!("`{key}` is not a string"))),
    };
    let home = match (named("store")?, named("server")?) {
        (Some(store), None) => Home::Store(store_for_repo(store, d)?),
        (None, Some(server)) => {
            let address = server_address(server).map_err(bad)?;
            let copy = copy_dir(&address, d)?;
            Home::Server { address, copy }
        }
        (Some(_), Some(_)) => return Err(bad("names both `store` and `server`: a project's grimoire is in one or the other".into())),
        (None, None) => return Err(bad("names neither `store` nor `server`".into())),
    };
    let name = match named("name")? {
        Some(name) if grimoire_core::store::is_slug(name) => name.to_string(),
        Some(name) => return Err(bad(format!("`name` is `{name}`, which is not a slug: lowercase letters and digits joined by single dashes"))),
        None => match named("store")?.filter(|s| is_bare_name(s)) {
            Some(store) => grimoire_core::scaffold::slugify(store),
            None => default_name(d),
        },
    };
    Ok(Some((home, name)))
}

/// The name a grimoire gets when none is given: its repository folder's, made a slug.
pub fn default_name(repo: &Path) -> String {
    let folder = repo.canonicalize().ok().and_then(|c| c.file_name().map(|n| n.to_string_lossy().to_string()));
    grimoire_core::scaffold::slugify(&folder.or_else(|| repo.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_default())
}

/// Whether a store argument is a bare name, one plain component, rather than a path.
fn is_bare_name(arg: &str) -> bool {
    let p = Path::new(arg);
    let mut parts = p.components();
    !p.is_absolute() && matches!((parts.next(), parts.next()), (Some(std::path::Component::Normal(_)), None)) && !arg.contains(['/', '\\'])
}

/// Walk up from `start` to the nearest `.grimoire.toml` and return the
/// directory holding it and the store its commands read: the store it names,
/// resolved, or this checkout's copy of the intent of the server it names.
pub fn find_project(start: &Path) -> Result<(PathBuf, PathBuf), StoreIoError> {
    let (dir, home) = find_home(start)?;
    Ok((dir, home.dir().to_path_buf()))
}

/// Write `.grimoire.toml` into a repository whose grimoire a team's server holds.
pub fn write_project_file_for_server(repo: &Path, address: &str, name: &str) -> Result<PathBuf, StoreIoError> {
    let f = repo.join(PROJECT_FILE);
    let text = format!(
        "# Written once by `grimoire init`. Names this project's grimoire and the server that holds it.\nname = {}\nserver = {}\n",
        toml::Value::String(name.to_string()),
        toml::Value::String(address.to_string())
    );
    std::fs::write(&f, text).map_err(|e| StoreIoError::Io(f.clone(), e))?;
    Ok(f)
}

/// Write `.grimoire.toml` into a repository.
pub fn write_project_file(repo: &Path, store: &str, name: &str) -> Result<PathBuf, StoreIoError> {
    let f = repo.join(PROJECT_FILE);
    let text = format!(
        "# Written once by `grimoire init`. Names this project's grimoire and the store that holds it.\nname = {}\nstore = {}\n",
        toml::Value::String(name.to_string()),
        toml::Value::String(store.to_string())
    );
    std::fs::write(&f, text).map_err(|e| StoreIoError::Io(f.clone(), e))?;
    Ok(f)
}

/// The store a project file or `init` names, resolved the one way both
/// use: an absolute path as it is, a path with a separator relative to the
/// repository, and a bare name under the stores directory. `.` and `..`
/// are paths, never names: a name is one plain component, joined to the
/// stores directory (where a link a person made there still leads where
/// they pointed it).
pub fn store_for_repo(arg: &str, repo: &Path) -> Result<PathBuf, StoreIoError> {
    let p = Path::new(arg);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else if is_bare_name(arg) {
        Ok(stores_dir()?.join(arg))
    } else {
        Ok(repo.join(arg))
    }
}

/// `path` made absolute with `.` and `..` resolved, through links as far as
/// it exists, so a path not created yet still resolves to where it would be.
fn resolved(path: &Path) -> std::io::Result<PathBuf> {
    let abs = std::path::absolute(path)?;
    let mut existing = abs.as_path();
    let mut rest = Vec::new();
    let base = loop {
        match existing.canonicalize() {
            Ok(c) => break c,
            Err(_) => match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_os_string());
                    existing = parent;
                }
                _ => break existing.to_path_buf(),
            },
        }
    };
    let mut out = base;
    for name in rest.into_iter().rev() {
        out.push(name);
    }
    // `absolute` keeps `..`; what did not exist cannot be a link, so it folds lexically.
    let mut folded = PathBuf::new();
    for c in out.components() {
        match c {
            std::path::Component::ParentDir => {
                folded.pop();
            }
            std::path::Component::CurDir => {}
            other => folded.push(other),
        }
    }
    Ok(folded)
}

/// Refuse a store that resolves inside `repo`, the repository it describes,
/// whether or not the store exists yet.
pub fn refuse_inside(store_dir: &Path, repo: &Path) -> Result<(), StoreIoError> {
    let store = resolved(store_dir).map_err(|e| StoreIoError::Io(store_dir.to_path_buf(), e))?;
    let repo_at = resolved(repo).map_err(|e| StoreIoError::Io(repo.to_path_buf(), e))?;
    if store.starts_with(&repo_at) {
        return Err(StoreIoError::InsideRepository(store_dir.to_path_buf(), repo.to_path_buf()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store_io::tests::temp;

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-store-inside-its-repository-is-refused", "checks": ["crates/server/src/store_io/location#outside-repository", "crates/server/src/store_io/location#resolution"]}
    /// ```
    #[test]
    fn a_store_inside_its_repository_is_refused() {
        let tmp = temp();
        let dir = tmp.path();
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let inside = |store: &Path| matches!(refuse_inside(store, &repo), Err(StoreIoError::InsideRepository(..)));
        assert!(inside(&repo.join("local-store")), "not created yet");
        assert!(inside(&repo.join("src")), "an existing folder in it");
        assert!(inside(&repo), "the repository itself");
        assert!(inside(&repo.join("src").join("..").join("store")), "through ..");
        assert!(inside(&dir.join("elsewhere").join("..").join("repo").join("store")), "back in through ..");
        assert!(refuse_inside(&dir.join("store"), &repo).is_ok(), "beside it");
        assert!(refuse_inside(&repo.join("..").join("store"), &repo).is_ok(), "beside it, through ..");
        assert!(refuse_inside(&dir.join("repo-store"), &repo).is_ok(), "a name that only starts like it");
        // Through a link: a folder outside the repository that leads into it is inside.
        let link = dir.join("link");
        if make_dir_link(&repo, &link) {
            assert!(inside(&link.join("store")), "through a link into the repository");
            assert!(refuse_inside(&dir.join("store"), &link).is_ok(), "a store beside the link's target is beside the repository");
        }
        // How a store argument resolves for a repository: a name only when it is one plain component.
        let stores = stores_dir().unwrap();
        assert_eq!(store_for_repo("mine", &repo).unwrap(), stores.join("mine"));
        for path in ["..", ".", "./mine", "../mine", "a/b"] {
            assert_eq!(store_for_repo(path, &repo).unwrap(), repo.join(path), "{path} is a path, never a name");
        }
        assert_eq!(store_for_repo(&dir.to_string_lossy(), &repo).unwrap(), dir);
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "project-file-round-trip", "checks": ["crates/server/src/store_io/location#resolution"]}
    /// ```
    #[test]
    fn project_file_round_trip() {
        let tmp = temp();
        let dir = tmp.path();
        let nested = dir.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        write_project_file(dir, "my-store", "my-store").unwrap();
        let (root, store) = find_project(&nested).unwrap();
        assert_eq!(root, dir);
        assert!(store.ends_with("my-store"));
        write_project_file(dir, "./local-store", "local").unwrap();
        let (_, store) = find_project(&nested).unwrap();
        assert_eq!(store, dir.join("./local-store"));
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-project-names-its-grimoire", "checks": ["crates/server/src/store_io/location#project-name"]}
    /// ```
    #[test]
    fn a_project_names_its_grimoire() {
        let tmp = temp();
        let dir = tmp.path().join("Billing Service");
        std::fs::create_dir_all(&dir).unwrap();
        let name = |text: &str| {
            std::fs::write(dir.join(PROJECT_FILE), text).unwrap();
            find_named(&dir).map(|(_, _, name)| name)
        };
        // Written by init, read back as given.
        write_project_file(&dir, "../stores/billing", "billing").unwrap();
        assert_eq!(find_named(&dir).unwrap().2, "billing");
        // Before names: the store's bare name, else the folder's, made slugs.
        assert_eq!(
            name(
                "store = \"Ledger\"
"
            )
            .unwrap(),
            "ledger"
        );
        assert_eq!(
            name(
                "store = \"../stores/ledger\"
"
            )
            .unwrap(),
            "billing-service"
        );
        assert_eq!(
            name(
                "server = \"https://grimoire.example\"
"
            )
            .unwrap(),
            "billing-service"
        );
        assert_eq!(default_name(&dir), "billing-service");
        // A name that is not a slug is refused, saying so.
        match name(
            "name = \"My Shop\"
store = \"shop\"
",
        ) {
            Err(StoreIoError::BadProjectFile(_, why)) => assert!(why.contains("not a slug"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    /// # Test contract
    ///
    /// ```json
    /// {"test": "a-project-names-a-store-or-a-server", "checks": ["crates/server/src/store_io/location#store-or-server"]}
    /// ```
    #[test]
    fn a_project_names_a_store_or_a_server() {
        let tmp = temp();
        let dir = tmp.path();
        let nested = dir.join("a");
        std::fs::create_dir_all(&nested).unwrap();
        // A server, kept without its trailing slash, and found from anywhere below.
        write_project_file_for_server(dir, &server_address("https://grimoire.example/").unwrap(), "shop").unwrap();
        let (root, home) = find_home(&nested).unwrap();
        let Home::Server { address, copy } = home.clone() else { panic!("{home:?}") };
        assert_eq!((root.as_path(), address.as_str()), (dir, "https://grimoire.example"));
        // Its store is this checkout's copy: under the copies, named for the server and the checkout.
        assert_eq!(copy.parent().unwrap(), copies_dir().unwrap());
        assert!(copy.file_name().unwrap().to_string_lossy().starts_with("grimoire-example-"), "{}", copy.display());
        assert_eq!(find_project(&nested).unwrap(), (dir.to_path_buf(), copy.clone()));
        assert_eq!(copy_dir(&address, dir).unwrap(), copy, "the same for the same checkout");
        assert_ne!(copy_dir(&address, &nested).unwrap(), copy, "another checkout has its own");
        assert_ne!(copy_dir("https://other.example", dir).unwrap(), copy, "and so has another server");
        // What is refused, naming the file.
        let refused = |text: &str| {
            std::fs::write(dir.join(PROJECT_FILE), text).unwrap();
            match find_home(dir) {
                Err(StoreIoError::BadProjectFile(f, why)) => {
                    assert_eq!(f, dir.join(PROJECT_FILE));
                    why
                }
                other => panic!("{text}: {other:?}"),
            }
        };
        assert!(refused("store = \"mine\"\nserver = \"https://grimoire.example\"\n").contains("both"));
        assert!(refused("# nothing named\n").contains("neither"));
        assert!(refused("server = \"grimoire.example\"\n").contains("http://"));
        assert!(refused("server = \"https://\"\n").contains("no host"));
        assert!(refused("server = 4242\n").contains("not a string"));
        for (given, kept) in [
            ("http://127.0.0.1:4242", "http://127.0.0.1:4242"),
            (" https://grimoire.example/team/ ", "https://grimoire.example/team"),
            ("https://grimoire.example/g/shop/", "https://grimoire.example/g/shop"),
            ("https://host/proxy/g/shop", "https://host/proxy/g/shop"),
        ] {
            assert_eq!(server_address(given).unwrap(), kept);
        }
        // A grimoire of a library is named last, by a slug, and an address carries no query or fragment.
        for refused in ["https://host/g/Not A Slug", "https://host/g/shop/more", "https://host/g/", "https://host/g/shop?x=1", "https://host#top"] {
            assert!(server_address(refused).is_err(), "{refused}");
        }
        assert_eq!(server_grimoire("https://host/proxy/g/shop").as_deref(), Some("shop"));
        assert_eq!(server_grimoire("https://grimoire.example/team"), None);
        // A person's tokens are kept outside any repository.
        assert!(!credentials_file().unwrap().starts_with(dir));
    }

    /// A link at `link` to the folder `target`: a junction on Windows, which
    /// needs no privilege, a symlink elsewhere. False when none can be made.
    fn make_dir_link(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(link).arg(target).output().is_ok_and(|o| o.status.success())
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }
}
