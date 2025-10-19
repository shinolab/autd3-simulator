use std::fs;
use std::io::Write;
use std::{collections::BTreeMap, path::Path, sync::Arc};

use cargo_about::{Krate, get_all_crates, licenses::config::Config};
use itertools::*;
use krates::{LockOptions, Utf8Path};
use regex::Regex;
use semver::Version;
use spdx::Licensee;

fn get_real_deps(dir: impl AsRef<Path>) -> anyhow::Result<Vec<(String, Version)>> {
    let cargo_tree = std::process::Command::new("cargo")
        .args(["tree", "--prefix", "none", "-e", "no-build", "-e", "no-dev"])
        .current_dir(dir)
        .output()?;

    if !cargo_tree.status.success() {
        anyhow::bail!(
            "cargo tree failed: {}",
            String::from_utf8_lossy(&cargo_tree.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&cargo_tree.stdout);
    stdout
        .lines()
        .map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                anyhow::bail!("unexpected line: {}", line);
            }
            Ok((parts[0].to_owned(), Version::parse(&parts[1][1..])?))
        })
        .collect::<Result<Vec<_>, _>>()
}

#[derive(serde::Deserialize, Debug)]
struct DepWorkaround {
    dep: Vec<Dep>,
}

#[derive(serde::Deserialize, Debug)]
struct Dep {
    name: String,
    path: Option<String>,
    tag: Option<String>,
}

impl DepWorkaround {
    fn load() -> anyhow::Result<Self> {
        Ok(toml::from_str(&fs::read_to_string("workaround.toml")?)?)
    }

    fn find(&self, writer: &mut impl Write, krate: &Krate) -> anyhow::Result<()> {
        if krate.name == "hexf-parse" && krate.version == Version::new(0, 2, 1) {
            return Ok(());
        }

        if let Some(d) = self.dep.iter().find(|d| d.name == krate.name) {
            let re = Regex::new(r"https://github.com/emilk/egui/tree/main/crates/(.*)").unwrap();

            let repo = if krate
                .repository
                .as_ref()
                .is_some_and(|r| re.captures(r).is_some())
            {
                "https://raw.githubusercontent.com/emilk/egui".to_string()
            } else {
                krate
                    .repository
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("no repository found for crate {}", krate.name))?
                    .trim_end_matches(".git")
                    .replace("github.com", "raw.githubusercontent.com")
            };
            let tag = d
                .tag
                .as_ref()
                .map(|tag| tag.replace("${VERSION}", &krate.version.to_string()));
            let path = if let Some(path) = &d.path {
                path
            } else {
                return Ok(());
            };
            let url = if let Some(tag) = tag {
                format!("{}/refs/tags/{}/{}", repo, tag, path)
            } else {
                format!("{}/refs/heads/master/{}", repo, path)
            };
            let res = reqwest::blocking::get(&url)?;
            if !res.status().is_success() {
                anyhow::bail!(
                    "failed to fetch license file from {}: {}",
                    url,
                    res.status()
                );
            }
            writeln!(writer)?;
            writeln!(writer, "{}", res.text()?)?;
        } else {
            anyhow::bail!("no workaround found for crate {}", krate.name);
        }
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let krates = get_all_crates(
        Utf8Path::from_path(root.join("Cargo.toml").as_path()).unwrap(),
        false,
        false,
        vec![],
        false,
        LockOptions {
            frozen: false,
            locked: true,
            offline: false,
        },
        &Config {
            ignore_build_dependencies: true,
            ignore_dev_dependencies: true,
            ..Default::default()
        },
        &[],
    )?;

    let store = cargo_about::licenses::store_from_cache()?;
    let client = reqwest::blocking::Client::new();
    let licenses = cargo_about::licenses::Gatherer::with_store(Arc::new(store)).gather(
        &krates,
        &Config {
            ignore_build_dependencies: true,
            ignore_dev_dependencies: true,
            ..Default::default()
        },
        Some(client),
    );

    let accepted = vec![
        Licensee::parse("MIT").unwrap(),
        Licensee::parse("Apache-2.0").unwrap(),
        Licensee::parse("Zlib").unwrap(),
        Licensee::parse("ISC").unwrap(),
        Licensee::parse("CC0-1.0").unwrap(),
        Licensee::parse("0BSD").unwrap(),
        Licensee::parse("Unicode-3.0").unwrap(),
        Licensee::parse("OFL-1.1").unwrap(),
        Licensee::parse("Ubuntu-font-1.0").unwrap(),
    ];
    let (_files, resolved) =
        cargo_about::licenses::resolution::resolve(&licenses, &accepted, &BTreeMap::new(), true);

    let real_deps = get_real_deps(&root)?;

    let mut file = std::fs::File::create(root.join("ThirdPartyNotices.txt"))?;
    let mut writer = std::io::BufWriter::new(&mut file);
    writeln!(writer, "THIRD-PARTY SOFTWARE NOTICES AND INFORMATION")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "This software includes the following third-party components."
    )?;
    writeln!(writer)?;
    writeln!(writer)?;

    let workaround = DepWorkaround::load()?;
    for (krate_license, resolved) in licenses.iter().zip(resolved.iter()) {
        if krate_license.krate.name.starts_with("autd3") {
            continue;
        }

        if !real_deps.iter().any(|(name, ver)| {
            name == &krate_license.krate.name && ver == &krate_license.krate.version
        }) {
            continue;
        }

        writeln!(writer)?;
        writeln!(
            writer,
            "---------------------------------------------------------"
        )?;
        writeln!(writer)?;
        writeln!(
            writer,
            "{} {} ({})",
            krate_license.krate.name,
            krate_license.krate.version,
            if let Some(r) = resolved {
                if r.licenses.is_empty() {
                    anyhow::bail!(
                        "no resolved licenses: {} {} ({:?})",
                        krate_license.krate.name,
                        krate_license.krate.version,
                        krate_license.lic_info
                    );
                } else {
                    r.licenses.iter().map(|f| format!("{f}")).join(" AND ")
                }
            } else {
                anyhow::bail!(
                    "no resolved licenses: {} {} ({:?})",
                    krate_license.krate.name,
                    krate_license.krate.version,
                    krate_license.lic_info
                );
            }
        )?;
        if let Some(repo) = &krate_license.krate.repository {
            writeln!(writer, "{}", repo)?;
        }

        let license_files = krate_license
            .license_files
            .iter()
            .filter(|lf| {
                lf.license_expr.evaluate(|req| {
                    resolved.as_ref().is_some_and(|resolved| {
                        resolved
                            .licenses
                            .iter()
                            .any(|lr| req.license.id() == lr.license.id())
                    })
                })
            })
            .collect::<Vec<_>>();
        if license_files.is_empty() {
            workaround.find(&mut writer, krate_license.krate)?;
        }
        for lf in &license_files {
            writeln!(writer)?;
            if license_files.len() > 1 {
                writeln!(writer, "---")?;
            }
            match &lf.kind {
                cargo_about::licenses::LicenseFileKind::Text(text) => {
                    writeln!(writer, "{}", text)?;
                }
                _ => {
                    anyhow::bail!("unexpected license file kind");
                }
            }
            if license_files.len() > 1 {
                writeln!(writer, "---")?;
            }
        }
        writeln!(
            writer,
            "---------------------------------------------------------"
        )?;
    }

    Ok(())
}
