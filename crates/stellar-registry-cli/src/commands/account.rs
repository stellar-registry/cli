//! Shared checks for the account commands. Since registry v0.8.0 an address
//! must consent to being named: by signing the naming call, or with an
//! earlier `preauthorize_account` for that name and owner.

use std::str::FromStr;

use stellar_cli::{config, xdr};
use stellar_registry_build::registry::Registry;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(
        "{account} has to approve being named `{name}`. {how}:\n  \
         stellar registry preauthorize-account --account-name {name} --account-address {account} --owner {owner}"
    )]
    NeedsConsent {
        account: String,
        name: String,
        owner: String,
        how: &'static str,
    },
    #[error(
        "{0} has to sign this. Run it with --source set to its identity, or pass its identity's alias as the address{1}"
    )]
    NeedsSigner(String, &'static str),
    #[error("{prefix}this needs {operator}'s signature, which the CLI doesn't have. {hint}")]
    OperatorMustSign {
        prefix: &'static str,
        operator: String,
        hint: &'static str,
    },
    #[error(transparent)]
    Config(#[from] config::Error),
    #[error(transparent)]
    Registry(#[from] stellar_registry_build::Error),
}

const SMART_ACCOUNT_NOTE: &str =
    ". A smart account (C-address) can't be signed for by the CLI; use its own wallet";

/// Resolve an identity alias or address to its strkey. Anything that isn't a
/// known alias is returned as given.
pub fn resolve_address(config: &config::Args, addr_or_alias: &str) -> String {
    if stellar_strkey::Contract::from_str(addr_or_alias).is_ok() {
        return addr_or_alias.to_string();
    }
    config::UnresolvedMuxedAccount::from_str(addr_or_alias)
        .ok()
        .and_then(|a| {
            a.resolve_muxed_account(&config.locator, config.hd_path())
                .ok()
        })
        .map_or_else(|| addr_or_alias.to_string(), |a| muxed_to_strkey(&a))
}

fn muxed_to_strkey(account: &xdr::MuxedAccount) -> String {
    let key = match account {
        xdr::MuxedAccount::Ed25519(xdr::Uint256(k)) => *k,
        xdr::MuxedAccount::MuxedEd25519(m) => m.ed25519.0,
    };
    stellar_strkey::ed25519::PublicKey(key)
        .to_string()
        .as_str()
        .to_string()
}

pub fn source_address(config: &config::Args) -> Result<String, Error> {
    Ok(muxed_to_strkey(&config.source_account()?))
}

/// Whether stellar-cli can sign for this address when it's passed as an
/// argument: it has a local identity (by alias, or matching a raw G-address).
fn has_local_signer(config: &config::Args, addr_or_alias: &str) -> bool {
    config::UnresolvedMuxedAccount::from_str(addr_or_alias)
        .ok()
        .and_then(|a| a.resolve_secret(&config.locator, config.hd_path()).ok())
        .is_some()
}

fn is_smart_account(address: &str) -> bool {
    stellar_strkey::Contract::from_str(address).is_ok()
}

/// Who has to authorize an owner-or-manager call (the manager if the registry
/// has one, otherwise the owner), and whether the CLI can sign for them.
pub struct Operator {
    pub address: String,
    pub managed: bool,
    can_sign: bool,
}

/// `owner_arg` is the owner as passed to the call, when the call takes it, so
/// stellar-cli can sign for it with a local identity.
pub async fn operator(
    registry: &Registry,
    config: &config::Args,
    owner: &str,
    owner_arg: Option<&str>,
) -> Result<Operator, Error> {
    let registry_manager = registry.manager().await?;
    let managed = registry_manager.is_some();
    let address = registry_manager.unwrap_or_else(|| owner.to_string());
    let can_sign = address == source_address(config)?
        || (!managed && owner_arg.is_some_and(|arg| has_local_signer(config, arg)));
    Ok(Operator {
        address,
        managed,
        can_sign,
    })
}

impl Operator {
    /// Fail a call the CLI can't get signed. For a real run, call this before
    /// submitting; for a dry run, after simulating, so the simulation result
    /// is still shown but the run doesn't read as a success.
    pub fn require_signer(&self, dry_run: bool) -> Result<(), Error> {
        if self.can_sign {
            return Ok(());
        }
        Err(Error::OperatorMustSign {
            prefix: if dry_run {
                "The dry run simulated, but it can't be submitted from here: "
            } else {
                ""
            },
            operator: self.address.clone(),
            hint: if self.managed {
                "This registry is managed, so its manager has to make this call, e.g. by \
                 executing a governance proposal."
            } else {
                "Run it with --source set to the owner's identity."
            },
        })
    }
}

/// What a naming call (register, update address, rename) is about to do.
pub struct Naming<'a> {
    /// Name as the registry stores it, without a channel prefix
    pub name: &'a str,
    /// Name as the user would type it, e.g. `unverified/alice`
    pub display_name: String,
    /// Address being named, as an alias or strkey
    pub account: &'a str,
    /// Owner the name will have, as a strkey
    pub owner: &'a str,
    /// Whether `account` is an argument of the call, so stellar-cli can sign
    /// for it with a local identity
    pub account_is_arg: bool,
}

/// Fail early, with instructions, when the named address won't have
/// consented by the time the call runs. Mirrors the registry's rule: consent
/// isn't needed from the operator (owner, or manager if set), and a
/// preauthorization counts only for the same owner.
pub async fn require_consent(
    registry: &Registry,
    config: &config::Args,
    operator: &Operator,
    naming: &Naming<'_>,
) -> Result<(), Error> {
    let account = resolve_address(config, naming.account);
    if account == operator.address {
        return Ok(());
    }
    if operator.managed {
        eprintln!(
            "Note: this registry is managed by {}. Its call (e.g. an executed governance \
             proposal) can only name {account} `{}` if {account} has preauthorized that name for \
             owner {}, or signs the call.",
            operator.address, naming.display_name, naming.owner
        );
    }
    if registry
        .account_preauth_owner(naming.name, &account)
        .await?
        .as_deref()
        == Some(naming.owner)
    {
        eprintln!(
            "Using {account}'s preauthorization for `{}`.",
            naming.display_name
        );
        return Ok(());
    }
    if account == source_address(config)?
        || (naming.account_is_arg && has_local_signer(config, naming.account))
    {
        return Ok(());
    }
    let how = if is_smart_account(&account) {
        "The CLI can't sign for a smart account, so it has to preauthorize from its own wallet, \
         the equivalent of"
    } else if naming.account_is_arg {
        "Pass its local identity's alias as the address so it signs this transaction, or have it \
         preauthorize first"
    } else {
        "This call can't carry its signature, so have it preauthorize first"
    };
    Err(Error::NeedsConsent {
        account,
        name: naming.display_name.clone(),
        owner: naming.owner.to_string(),
        how,
    })
}

/// Fail early when `addr_or_alias` has to sign a call it's an argument of
/// but stellar-cli has no key for it.
pub fn require_signer(config: &config::Args, addr_or_alias: &str) -> Result<(), Error> {
    let address = resolve_address(config, addr_or_alias);
    if address == source_address(config)? || has_local_signer(config, addr_or_alias) {
        return Ok(());
    }
    let note = if is_smart_account(&address) {
        SMART_ACCOUNT_NOTE
    } else {
        ""
    };
    Err(Error::NeedsSigner(address, note))
}

#[cfg(feature = "integration-tests")]
#[cfg(test)]
mod tests {
    use stellar_cli::commands::contract::deploy::wasm;
    use stellar_registry_build::registry::Registry;
    use stellar_registry_test::{AssertExt, RegistryTest};

    fn fetch(registry: &RegistryTest, cmd: &str, name: &str) -> String {
        registry
            .registry_cli(cmd)
            .arg(name)
            .assert()
            .success()
            .stdout_as_str()
    }

    fn alice(registry: &RegistryTest) -> String {
        registry.alice_address.to_string().as_str().to_string()
    }

    /// An address no local identity has a key for.
    fn stranger() -> String {
        stellar_strkey::ed25519::PublicKey([7; 32])
            .to_string()
            .as_str()
            .to_string()
    }

    // `unverified` is unmanaged: the owner signs, and so must the account.
    #[tokio::test]
    async fn unmanaged_register_needs_the_account_to_sign() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;

        let refused = registry
            .registry_cli("register-account")
            .args(["--account-name", "unverified/stranger"])
            .args(["--account-address", &stranger()])
            .assert()
            .failure()
            .stderr_as_str();
        assert!(refused.contains("preauthorize-account"), "{refused}");

        // An alias with a local key signs the transaction.
        registry
            .registry_cli("register-account")
            .args(["--account-name", "unverified/bob"])
            .args(["--account-address", "bob"])
            .assert()
            .success();
        assert_eq!(fetch(&registry, "fetch-account-id", "unverified/bob"), bob);
        assert_eq!(
            fetch(&registry, "fetch-account-owner", "unverified/bob"),
            alice(&registry)
        );
    }

    // The root registry in tests is managed by alice, standing in for
    // governance: it can't collect bob's signature, so bob preauthorizes.
    #[tokio::test]
    async fn managed_register_uses_preauthorization() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;
        let alice = alice(&registry);

        registry
            .registry_cli("preauthorize-account")
            .args(["--account-name", "bob", "--account-address", "bob"])
            .args(["--owner", &alice, "--source", "bob"])
            .assert()
            .success();

        let out = registry
            .registry_cli("register-account")
            .args(["--account-name", "bob", "--account-address", &bob])
            .assert()
            .success()
            .stderr_as_str();
        assert!(out.contains("managed by"), "{out}");
        assert!(out.contains("Using"), "{out}");
        assert_eq!(fetch(&registry, "fetch-account-id", "bob"), bob);
    }

    // Bob isn't the root's manager, so neither a dry run nor a real run may
    // read as a success.
    #[tokio::test]
    async fn managed_registry_without_the_manager_key_is_refused() {
        let registry = RegistryTest::new().await;
        registry.generate_funded_identity("bob").await;
        for dry_run in [true, false] {
            let mut cmd = registry.registry_cli("register-account");
            cmd.args([
                "--account-name",
                "bob",
                "--account-address",
                "bob",
                "--source",
                "bob",
            ]);
            if dry_run {
                cmd.arg("--dry-run");
            }
            let refused = cmd.assert().failure().stderr_as_str();
            assert!(
                refused.contains("its manager has to make this call"),
                "{refused}"
            );
            assert!(!refused.contains("Successfully"), "{refused}");
            assert_eq!(refused.contains("dry run simulated"), dry_run, "{refused}");
        }
    }

    #[tokio::test]
    async fn unmanaged_calls_need_the_owner_key() {
        let registry = RegistryTest::new().await;
        registry.generate_funded_identity("bob").await;
        registry
            .registry_cli("register-account")
            .args([
                "--account-name",
                "unverified/bob",
                "--account-address",
                "bob",
            ])
            .assert()
            .success();

        // alice owns it; bob can't sign as owner.
        let refused = registry
            .registry_cli("flag-account")
            .args(["--account-name", "unverified/bob", "--source", "bob"])
            .assert()
            .failure()
            .stderr_as_str();
        assert!(refused.contains("--source set to the owner"), "{refused}");
    }

    #[tokio::test]
    async fn revoked_preauthorization_is_gone() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;
        let alice = alice(&registry);
        let preauth = |cmd: &str| {
            let mut c = registry.registry_cli(cmd);
            c.args([
                "--account-name",
                "bob",
                "--account-address",
                "bob",
                "--source",
                "bob",
            ]);
            c
        };

        preauth("preauthorize-account")
            .args(["--owner", &alice])
            .assert()
            .success();
        let config = registry
            .parse_cmd::<crate::commands::fetch_account_owner::Cmd>(&["bob"])
            .unwrap()
            .config;
        let root = Registry::new(&config, None).await.unwrap();
        assert_eq!(
            root.account_preauth_owner("bob", &bob).await.unwrap(),
            Some(alice.clone())
        );

        preauth("revoke-account-preauth").assert().success();
        assert_eq!(root.account_preauth_owner("bob", &bob).await.unwrap(), None);
    }

    #[tokio::test]
    async fn rename_and_update_need_consent() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;
        let carol = registry.generate_funded_identity("carol").await;
        let alice = alice(&registry);

        registry
            .registry_cli("register-account")
            .args([
                "--account-name",
                "unverified/bob",
                "--account-address",
                "bob",
            ])
            .assert()
            .success();

        // rename_account doesn't take the address, so bob can't co-sign it.
        let refused = registry
            .registry_cli("rename-account")
            .args(["--account-name", "unverified/bob", "--new-name", "robert"])
            .assert()
            .failure()
            .stderr_as_str();
        assert!(refused.contains("preauthorize-account"), "{refused}");

        registry
            .registry_cli("preauthorize-account")
            .args([
                "--account-name",
                "unverified/robert",
                "--account-address",
                "bob",
            ])
            .args(["--owner", &alice, "--source", "bob"])
            .assert()
            .success();
        registry
            .registry_cli("rename-account")
            .args(["--account-name", "unverified/bob", "--new-name", "robert"])
            .assert()
            .success();
        assert_eq!(
            fetch(&registry, "fetch-account-id", "unverified/robert"),
            bob
        );

        registry
            .registry_cli("update-account-address")
            .args([
                "--account-name",
                "unverified/robert",
                "--new-address",
                "carol",
            ])
            .assert()
            .success();
        assert_eq!(
            fetch(&registry, "fetch-account-id", "unverified/robert"),
            carol
        );
    }

    #[tokio::test]
    async fn flagged_account_needs_force() {
        let registry = RegistryTest::new().await;
        let bob = registry.generate_funded_identity("bob").await;
        registry
            .registry_cli("register-account")
            .args([
                "--account-name",
                "unverified/bob",
                "--account-address",
                "bob",
            ])
            .assert()
            .success();
        registry
            .registry_cli("flag-account")
            .args(["--account-name", "unverified/bob"])
            .assert()
            .success();

        registry
            .registry_cli("fetch-account-id")
            .arg("unverified/bob")
            .assert()
            .failure();
        // The library refuses too, so callers other than the CLI get the check.
        let config = registry
            .parse_cmd::<crate::commands::fetch_account_owner::Cmd>(&["unverified/bob"])
            .unwrap()
            .config;
        let unverified = Registry::new(&config, Some("unverified")).await.unwrap();
        assert!(matches!(
            unverified.fetch_account_id("bob").await,
            Err(stellar_registry_build::Error::AccountFlagged(_))
        ));
        assert_eq!(
            unverified.fetch_account_id_unchecked("bob").await.unwrap(),
            bob
        );
        let forced = registry
            .registry_cli("fetch-account-id")
            .args(["unverified/bob", "--force"])
            .assert()
            .success()
            .stdout_as_str();
        assert_eq!(forced, bob);

        registry
            .registry_cli("flag-account")
            .args(["--account-name", "unverified/bob", "--unflag"])
            .assert()
            .success();
        assert_eq!(fetch(&registry, "fetch-account-id", "unverified/bob"), bob);
    }

    // The CLI can't sign for a smart account, so it says so before
    // submitting anything.
    #[tokio::test]
    async fn smart_account_needs_its_own_wallet() {
        let registry = RegistryTest::new().await;
        let v1 = registry.hello_wasm_v1();
        let deploy = registry
            .parse_cmd::<wasm::Cmd>(&[
                "--wasm",
                v1.to_str().unwrap(),
                "--source",
                "alice",
                "--fee=10000000",
                "--",
                "--admin=alice",
            ])
            .unwrap();
        let contract_id = deploy
            .execute(&deploy.config, false, false)
            .await
            .unwrap()
            .into_result()
            .unwrap()
            .to_string();

        let refused = registry
            .registry_cli("register-account")
            .args(["--account-name", "unverified/smart"])
            .args(["--account-address", &contract_id])
            .assert()
            .failure()
            .stderr_as_str();
        assert!(refused.contains("smart account"), "{refused}");
    }
}
