#![allow(dead_code)]

use base64::{engine::general_purpose::STANDARD, Engine};
use isolock_cas::{FileStore, Sha256Digest};
use isolock_plan::profile::{
    ArchiveFormat, CargoPolicy, CatalogEntry, Profile, ProfileManifest, Provider, ToolArtifact,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const PLATFORM: &str = "linux-64";
pub const RUST_PROFILE: &str = "rust-pixi";

pub struct Fixture {
    pub temp: tempfile::TempDir,
    pub root: PathBuf,
    pub profiles: BTreeMap<String, Profile>,
    pub blobs: BTreeMap<String, Vec<u8>>,
    pub catalog: Vec<CatalogEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Configuration<'a> {
    schema_version: u32,
    profiles: &'a BTreeMap<String, Profile>,
    storage: BTreeMap<String, String>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    #[allow(clippy::too_many_lines)] // Declarative multi-ecosystem offline fixture data.
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project with ' quote");
        fs::create_dir_all(&root).unwrap();
        let mut fixture = Self {
            temp,
            root,
            profiles: BTreeMap::new(),
            blobs: BTreeMap::new(),
            catalog: Vec::new(),
        };
        let cargo_bytes = archive(
            &[
                (
                    "hello-1.0.0/Cargo.toml",
                    "[package]\nname=\"hello\"\nversion=\"1.0.0\"\nedition=\"2021\"\n",
                    false,
                ),
                (
                    "hello-1.0.0/src/lib.rs",
                    "pub fn hello() -> u8 { 42 }\n",
                    false,
                ),
            ],
            "-czf",
        );
        let cargo_digest = fixture.blob("cargo", &cargo_bytes);
        fs::write(fixture.root.join("Cargo.lock"), format!("version = 4\n[[package]]\nname = \"app\"\nversion = \"0.1.0\"\ndependencies = [\"hello\"]\n[[package]]\nname = \"hello\"\nversion = \"1.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"{}\"\n", cargo_digest.as_str())).unwrap();
        fs::write(fixture.root.join("Cargo.toml"), "[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[dependencies]\nhello=\"=1.0.0\"\n").unwrap();
        fs::create_dir(fixture.root.join("src")).unwrap();
        fs::write(
            fixture.root.join("src/main.rs"),
            "fn main() { println!(\"{}\", hello::hello()); }\n",
        )
        .unwrap();
        let rust_script = "#!/bin/sh\n[ \"${CARGO_NET_OFFLINE:-}\" = true ] || { echo not-offline >&2; exit 1; }\n[ -f \"$CARGO_HOME/config.toml\" ] || exit 1\ngrep -q isolock-vendor \"$CARGO_HOME/config.toml\" || exit 1\nprintf 'cargo fixture: built hello offline\\n'\n";
        let rust_archive = archive(
            &[
                ("bin/cargo", rust_script, true),
                ("bin/rustc", "#!/bin/sh\necho rustc-fixture\n", true),
            ],
            "-cjf",
        );
        let conda_rust = fixture.blob("conda-rust", &rust_archive);
        let python_archive = archive(
            &[(
                "bin/python",
                "#!/bin/sh\nprintf 'python fixture %s\\n' \"$*\"\n",
                true,
            )],
            "-cjf",
        );
        let conda_python = fixture.blob("conda-python", &python_archive);
        let bun_archive = archive(&[("bin/bun", "#!/bin/sh\necho conda-bun\n", true)], "-cjf");
        let conda_bun = fixture.blob("conda-bun", &bun_archive);
        let source = "https://channel.invalid/selected";
        let mut lock = String::from("version: 6\nenvironments:\n");
        for (environment, name) in [("rust", "rust"), ("python", "python"), ("ts", "bun")] {
            lock.push_str(&format!("  {environment}:\n    channels:\n    - url: {source}/\n    packages:\n      linux-64:\n      - conda: {source}/linux-64/{name}-1.0.0-0.tar.bz2\n"));
        }
        lock.push_str("packages:\n");
        for (name, digest, size) in [
            ("rust", &conda_rust, rust_archive.len()),
            ("python", &conda_python, python_archive.len()),
            ("bun", &conda_bun, bun_archive.len()),
        ] {
            lock.push_str(&format!(
                "- conda: {source}/linux-64/{name}-1.0.0-0.tar.bz2\n  sha256: {}\n  size: {size}\n",
                digest.as_str()
            ));
        }
        fs::write(fixture.root.join("pixi.lock"), lock).unwrap();
        fs::write(fixture.root.join("pixi.toml"), format!("[workspace]\nname=\"fixture\"\nchannels=[\"{source}\"]\nplatforms=[\"linux-64\"]\n[feature.rust.dependencies]\nrust=\"==1.0.0\"\n[feature.python.dependencies]\npython=\"==1.0.0\"\n[feature.ts.dependencies]\nbun=\"==1.0.0\"\n[environments]\nrust=[\"rust\"]\npython=[\"python\"]\nts=[\"ts\"]\n")).unwrap();
        let isolock_seed = fixture.tool(
            "isolock-seed",
            "isolock",
            b"#!/bin/sh\necho verified-isolock\n",
            ArchiveFormat::Raw,
        );
        let record =
            serde_json::json!({"name":"rust", "version":"1.0.0", "build":"0", "sha256":conda_rust});
        let fake_pixi = format!("#!/bin/sh\nset -eu\n[ \"$1\" = install ] || exit 31\nshift\nenv= manifest= locked= offline=\nwhile [ \"$#\" -gt 0 ]; do case \"$1\" in --environment) shift; env=$1;; --manifest-path) shift; manifest=$1;; --config-file) shift;; --locked) locked=1;; --offline) offline=1;; *) exit 32;; esac; shift; done\n[ \"$env/$locked/$offline\" = rust/1/1 ] || exit 33\nbase=$(dirname \"$manifest\")\ngrep -q file:// \"$base/pixi.lock\" || exit 34\n! grep -q channel.invalid \"$base/pixi.lock\" || exit 35\nmkdir -p \"$base/.pixi/envs/rust/bin\" \"$base/.pixi/envs/rust/conda-meta\"\nprintf '%s\\n' '{record}' > \"$base/.pixi/envs/rust/conda-meta/rust.json\"\ncat > \"$base/.pixi/envs/rust/bin/cargo\" <<'CARGO'\n{rust_script}CARGO\nprintf '#!/bin/sh\\necho rustc-fixture\\n' > \"$base/.pixi/envs/rust/bin/rustc\"\nchmod +x \"$base/.pixi/envs/rust/bin/cargo\" \"$base/.pixi/envs/rust/bin/rustc\"\n");
        let pixi_seed = fixture.tool(
            "pixi-seed",
            "pixi",
            fake_pixi.as_bytes(),
            ArchiveFormat::Raw,
        );
        let cargo = CargoPolicy {
            cargo_version: "1.88.0".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            ..CargoPolicy::default()
        };
        let pixi = Profile {
            provider: Provider::Pixi,
            platforms: vec![PLATFORM.into()],
            environment: Some("rust".into()),
            pixi_lock: Some("pixi.lock".into()),
            pixi_manifest: Some("pixi.toml".into()),
            cargo_lock: Some("Cargo.lock".into()),
            seeds: BTreeMap::from([
                ("isolock".into(), isolock_seed.clone()),
                ("pixi".into(), pixi_seed),
            ]),
            cargo: cargo.clone(),
            ..Profile::default()
        };
        fixture.profiles.insert(RUST_PROFILE.into(), pixi);
        let rust = fixture.tool(
            "rust-runtime",
            "rust-prefix.tar.bz2",
            &rust_archive,
            ArchiveFormat::Tar,
        );
        fixture.profiles.insert(
            "rust-standalone".into(),
            Profile {
                platforms: vec![PLATFORM.into()],
                cargo_lock: Some("Cargo.lock".into()),
                cargo,
                seeds: BTreeMap::from([("isolock".into(), isolock_seed.clone())]),
                runtimes: BTreeMap::from([("rust".into(), rust)]),
                ..Profile::default()
            },
        );
        let python = fixture.tool(
            "python-runtime",
            "python-prefix.tar.bz2",
            &python_archive,
            ArchiveFormat::Tar,
        );
        let wheel = fixture.blob("wheel", b"reviewed wheel fixture");
        fs::write(
            fixture.root.join("requirements.lock"),
            format!("demo==1.0.0 --hash=sha256:{}\n", wheel.as_str()),
        )
        .unwrap();
        fixture.catalog.push(CatalogEntry {
            name: "demo".into(),
            version: "1.0.0".into(),
            platform: PLATFORM.into(),
            digest: wheel,
            filename: "demo-1.0.0-py3-none-any.whl".into(),
            source: "https://files.pythonhosted.org/demo-1.0.0-py3-none-any.whl".into(),
            integrity: None,
        });
        fixture.profiles.insert(
            "python-standalone".into(),
            Profile {
                platforms: vec![PLATFORM.into()],
                pypi_lock: Some("requirements.lock".into()),
                catalog: Some("catalog.json".into()),
                seeds: BTreeMap::from([("isolock".into(), isolock_seed.clone())]),
                runtimes: BTreeMap::from([("python".into(), python)]),
                ..Profile::default()
            },
        );
        let js_archive = archive(
            &[
                (
                    "package/package.json",
                    "{\"name\":\"hello-cli\",\"version\":\"1.0.0\",\"bin\":{\"hello\":\"cli.sh\"}}",
                    false,
                ),
                ("package/cli.sh", "#!/bin/sh\necho locked-hello\n", true),
            ],
            "-czf",
        );
        let js = fixture.blob("javascript", &js_archive);
        let digest_bytes = (0..64)
            .step_by(2)
            .map(|index| u8::from_str_radix(&js.as_str()[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let integrity = format!("sha256-{}", STANDARD.encode(digest_bytes));
        fixture.catalog.push(CatalogEntry {
            name: "hello-cli".into(),
            version: "1.0.0".into(),
            platform: PLATFORM.into(),
            digest: js,
            filename: "hello-cli-1.0.0.tgz".into(),
            source: "https://registry.npmjs.org/hello-cli/-/hello-cli-1.0.0.tgz".into(),
            integrity: Some(integrity.clone()),
        });
        fs::write(
            fixture.root.join("package.json"),
            "{\"name\":\"app\",\"version\":\"1.0.0\",\"dependencies\":{\"hello-cli\":\"1.0.0\"}}",
        )
        .unwrap();
        fs::write(fixture.root.join("package-lock.json"), serde_json::to_vec(&serde_json::json!({"name":"app","version":"1.0.0","lockfileVersion":3,"packages":{"":{"name":"app","version":"1.0.0","dependencies":{"hello-cli":"1.0.0"}},"node_modules/hello-cli":{"version":"1.0.0","resolved":"https://registry.npmjs.org/hello-cli/-/hello-cli-1.0.0.tgz","integrity":integrity,"bin":{"hello":"cli.sh"}}}})).unwrap()).unwrap();
        let bun = fixture.tool(
            "bun-runtime",
            "bun",
            b"#!/bin/sh\nprintf 'bun-runtime %s\\n' \"$*\"\n",
            ArchiveFormat::Raw,
        );
        fixture.profiles.insert(
            "bun-standalone".into(),
            Profile {
                platforms: vec![PLATFORM.into()],
                javascript_lock: Some("package-lock.json".into()),
                catalog: Some("catalog.json".into()),
                seeds: BTreeMap::from([("isolock".into(), isolock_seed)]),
                runtimes: BTreeMap::from([("bun".into(), bun)]),
                ..Profile::default()
            },
        );
        fixture.profiles.insert(
            "all".into(),
            Profile {
                provider: Provider::Composed,
                platforms: vec![PLATFORM.into()],
                compose: vec![
                    RUST_PROFILE.into(),
                    "python-standalone".into(),
                    "bun-standalone".into(),
                ],
                ..Profile::default()
            },
        );
        fixture.write_catalog();
        fixture.write_config(None);
        fixture
    }

    pub fn cas(&self) -> FileStore {
        FileStore::new(self.root.join(".isolock/cas"))
    }
    pub fn blob(&mut self, name: &str, bytes: &[u8]) -> Sha256Digest {
        let digest = Sha256Digest::from_bytes(bytes);
        self.cas().put_bytes(&digest, bytes).unwrap();
        self.blobs.insert(name.into(), bytes.into());
        digest
    }
    pub fn tool(
        &mut self,
        key: &str,
        filename: &str,
        bytes: &[u8],
        format: ArchiveFormat,
    ) -> ToolArtifact {
        ToolArtifact {
            version: "1.0.0".into(),
            platform: PLATFORM.into(),
            digest: self.blob(key, bytes),
            filename: filename.into(),
            source: format!("https://github.com/fixture/releases/{filename}"),
            format,
            entry: None,
        }
    }
    pub fn write_catalog(&self) {
        fs::write(
            self.root.join("catalog.json"),
            serde_json::to_vec_pretty(&self.catalog).unwrap(),
        )
        .unwrap();
    }
    pub fn write_config(&self, source: Option<&Path>) {
        let mut storage = BTreeMap::new();
        if let Some(source) = source {
            storage.insert("source".into(), source.to_str().unwrap().into());
        }
        fs::write(
            self.root.join("isolock.toml"),
            toml::to_string(&Configuration {
                schema_version: 1,
                profiles: &self.profiles,
                storage,
            })
            .unwrap(),
        )
        .unwrap();
    }
    pub fn command(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_isolock"));
        command
            .args(arguments)
            .current_dir(&self.root)
            .env("HOME", self.temp.path().join("empty-home"))
            .env("XDG_CONFIG_HOME", self.temp.path().join("empty-config"));
        for (key, _) in std::env::vars().filter(|(key, _)| key.starts_with("ISOLOCK_")) {
            command.env_remove(key);
        }
        command
    }
    pub fn run(&self, arguments: &[&str]) -> Output {
        self.command(arguments).output().unwrap()
    }
    pub fn success(&self, arguments: &[&str]) -> Output {
        let output = self.run(arguments);
        assert!(output.status.success(), "{}", text(&output));
        output
    }
    pub fn plan(&self, profile: &str) -> ProfileManifest {
        self.success(&["plan", "--profile", profile, "--platform", PLATFORM]);
        self.manifest(profile)
    }
    pub fn sync(&self, profile: &str) {
        self.success(&[
            "sync",
            "--profile",
            profile,
            "--platform",
            PLATFORM,
            "--locked",
            "--offline",
        ]);
    }
    pub fn manifest(&self, profile: &str) -> ProfileManifest {
        serde_json::from_slice(
            &fs::read(
                self.root
                    .join(format!(".isolock/plans/{profile}-{PLATFORM}.lock")),
            )
            .unwrap(),
        )
        .unwrap()
    }
    pub fn receipt(&self, profile: &str) -> serde_json::Value {
        serde_json::from_slice(
            &fs::read(
                self.root
                    .join(format!(".isolock/receipts/{profile}-{PLATFORM}.json")),
            )
            .unwrap(),
        )
        .unwrap()
    }
    pub fn bin(&self, profile: &str, program: &str) -> PathBuf {
        self.root.join(format!(
            ".isolock/profiles/{profile}/{PLATFORM}/bin/{program}"
        ))
    }
}

pub fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub fn archive(files: &[(&str, &str, bool)], mode: &str) -> Vec<u8> {
    let temp = tempfile::tempdir().unwrap();
    let content = temp.path().join("content");
    fs::create_dir(&content).unwrap();
    for (name, data, executable) in files {
        let path = content.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, data).unwrap();
        fs::set_permissions(
            &path,
            fs::Permissions::from_mode(if *executable { 0o755 } else { 0o644 }),
        )
        .unwrap();
    }
    let path = temp.path().join("archive");
    assert!(Command::new("tar")
        .env_remove("TAR_OPTIONS")
        .args([mode])
        .arg(&path)
        .arg("-C")
        .arg(content)
        .arg(".")
        .status()
        .unwrap()
        .success());
    fs::read(path).unwrap()
}
