use stellar_cli::config;

use crate::{
    Error,
    contract::{Contract, PreHashContractID},
    name,
};

pub struct Registry(Contract);

impl Registry {
    pub async fn from_named_registry(
        config: &config::Args,
        name: &name::Prefixed,
    ) -> Result<Self, Error> {
        Self::new(config, name.channel()).await
    }

    pub async fn new(config: &config::Args, name: Option<&str>) -> Result<Self, Error> {
        let contract = Self::verified(config)?;
        Ok(if let Some(name) = name {
            if let Ok(contract_id) = name.parse() {
                Registry(Contract::new(contract_id, config))
            } else {
                contract.fetch_contract(name).await.map(Registry)?
            }
        } else {
            contract
        })
    }

    /// Fetch the deployed contract id for `name`, refusing to return it if the
    /// contract is flagged as compromised in the registry. Callers that really
    /// want a flagged id (e.g. behind a user-facing `--force`) must say so with
    /// [`Self::fetch_contract_id_unchecked`].
    pub async fn fetch_contract_id(&self, name: &str) -> Result<stellar_strkey::Contract, Error> {
        if self.is_contract_flagged(name).await? {
            return Err(Error::ContractFlagged(name.to_string()));
        }
        self.fetch_contract_id_unchecked(name).await
    }

    /// Fetch the deployed contract id for `name` without the compromised-flag
    /// check. Dangerous: prefer [`Self::fetch_contract_id`].
    pub async fn fetch_contract_id_unchecked(
        &self,
        name: &str,
    ) -> Result<stellar_strkey::Contract, Error> {
        let slop = ["fetch_contract_id", "--contract-name", name];
        let contract_id = self.0.invoke_with_result(&slop, true).await?;
        contract_id
            .trim_matches('"')
            .parse()
            .map_err(|_| Error::InvalidContractId(contract_id))
    }

    pub async fn fetch_contract(&self, name: &str) -> Result<Contract, Error> {
        // Unchecked on purpose: this resolves channel/subregistry contracts
        // during `Registry::new`, before any user-facing `--force` flag can be
        // consulted. The flagged-contract rejection is scoped to leaf
        // contract-id lookups via `fetch_contract_id`.
        Ok(Contract::new(
            self.fetch_contract_id_unchecked(name).await?,
            self.0.config(),
        ))
    }

    /// Is the named contract flagged as compromised in this (sub)registry?
    ///
    /// There is no on-chain getter for the flag, so read the raw persistent
    /// `ContractEntry` ledger entry directly. It is keyed by
    /// `(Symbol("CR"), <canonical name>)` and stored as a 2-tuple when
    /// unflagged and a 3-tuple (with a trailing `Void` sentinel) when flagged —
    /// the vec length carries the flag. Mirrors the registry contract's
    /// `ContractKey` / `ContractEntry` (stellar-registry/contracts
    /// `src/storage.rs`); coupled to that encoding by design.
    pub async fn is_contract_flagged(&self, name: &str) -> Result<bool, Error> {
        self.is_entry_flagged("CR", name).await
    }

    /// Is the named account flagged as compromised in this (sub)registry?
    /// Same encoding as contracts, under the registry's `AccountKey` ("AC").
    pub async fn is_account_flagged(&self, name: &str) -> Result<bool, Error> {
        self.is_entry_flagged("AC", name).await
    }

    async fn is_entry_flagged(&self, prefix: &str, name: &str) -> Result<bool, Error> {
        Ok(self
            .contract_data(
                entry_key(prefix, name)?,
                stellar_cli::xdr::ContractDataDurability::Persistent,
            )
            .await?
            .is_some_and(|val| is_flagged_entry(&val)))
    }

    async fn contract_data(
        &self,
        key: stellar_cli::xdr::ScVal,
        durability: stellar_cli::xdr::ContractDataDurability,
    ) -> Result<Option<stellar_cli::xdr::ScVal>, Error> {
        use stellar_cli::xdr;
        let ledger_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
            contract: self.0.sc_address(),
            key,
            durability,
        });
        let entries = self
            .0
            .rpc_client()?
            .get_full_ledger_entries(&[ledger_key])
            .await?
            .entries;
        Ok(entries.into_iter().find_map(|e| match e.val {
            xdr::LedgerEntryData::ContractData(cd) => Some(cd.val),
            _ => None,
        }))
    }

    /// The registry's manager, if it has one (a *managed* registry).
    pub async fn manager(&self) -> Result<Option<String>, Error> {
        let manager = self.0.invoke_with_result(&["manager"], true).await?;
        let manager = manager.trim();
        Ok((manager != "null").then(|| manager.trim_matches('"').to_string()))
    }

    /// Fetch the address `name` points at, refusing to return it if the
    /// account is flagged as compromised. Callers that really want a flagged
    /// address must say so with [`Self::fetch_account_id_unchecked`].
    pub async fn fetch_account_id(&self, name: &str) -> Result<String, Error> {
        if self.is_account_flagged(name).await? {
            return Err(Error::AccountFlagged(name.to_string()));
        }
        self.fetch_account_id_unchecked(name).await
    }

    /// Fetch the address `name` points at without the compromised-flag check.
    /// Dangerous: prefer [`Self::fetch_account_id`].
    pub async fn fetch_account_id_unchecked(&self, name: &str) -> Result<String, Error> {
        let slop = ["fetch_account_id", "--account-name", name];
        Ok(self
            .0
            .invoke_with_result(&slop, true)
            .await?
            .trim_matches('"')
            .to_string())
    }

    pub async fn fetch_contract_owner(&self, name: &str) -> Result<String, Error> {
        let slop = ["fetch_contract_owner", "--contract-name", name];
        Ok(self
            .0
            .invoke_with_result(&slop, true)
            .await?
            .trim_matches('"')
            .to_string())
    }

    pub async fn fetch_account_owner(&self, name: &str) -> Result<String, Error> {
        let slop = ["fetch_account_owner", "--account-name", name];
        Ok(self
            .0
            .invoke_with_result(&slop, true)
            .await?
            .trim_matches('"')
            .to_string())
    }

    /// The owner `address` preauthorized being named `name` under, if it has
    /// an unexpired `preauthorize_account` here. There is no getter, so this
    /// reads the temporary entry the registry keys by `(<canonical name>,
    /// address)` (stellar-registry/contracts `preauth_accounts`).
    pub async fn account_preauth_owner(
        &self,
        name: &str,
        address: &str,
    ) -> Result<Option<String>, Error> {
        use stellar_cli::xdr;
        Ok(
            match self
                .contract_data(
                    account_preauth_key(name, address)?,
                    xdr::ContractDataDurability::Temporary,
                )
                .await?
            {
                Some(xdr::ScVal::Address(owner)) => Some(owner.to_string()),
                _ => None,
            },
        )
    }

    pub fn as_contract(&self) -> &Contract {
        &self.0
    }

    pub fn verified(config: &config::Args) -> Result<Self, Error> {
        Ok(Registry(Contract::new(
            if let Ok(id) = std::env::var("STELLAR_REGISTRY_CONTRACT_ID") {
                id.parse().map_err(|_| Error::InvalidContractId(id))?
            } else {
                verified_contract_id(&config.get_network()?.network_passphrase)
            },
            config,
        )))
    }
}

/// Key of a contract (`"CR"`) or account (`"AC"`) entry in a registry's
/// persistent storage: `(Symbol(prefix), <canonical name>)`. Mirrors the
/// registry contract's `ContractKey` / `AccountKey`.
fn entry_key(prefix: &str, name: &str) -> Result<stellar_cli::xdr::ScVal, Error> {
    use stellar_cli::xdr;
    let canonical = name::canonicalize(name);
    Ok(xdr::ScVal::Vec(Some(
        vec![
            xdr::ScVal::Symbol(xdr::ScSymbol(prefix.try_into()?)),
            xdr::ScVal::String(xdr::ScString(canonical.as_str().try_into()?)),
        ]
        .try_into()?,
    )))
}

/// Entries are a 2-tuple when unflagged and a 3-tuple (trailing `Void`) when
/// flagged; the length carries the flag.
fn is_flagged_entry(val: &stellar_cli::xdr::ScVal) -> bool {
    matches!(val, stellar_cli::xdr::ScVal::Vec(Some(v)) if v.len() == 3)
}

/// Key of an account preauthorization in a registry's temporary storage:
/// `(<canonical name>, address)`. Mirrors the contract's `preauth_accounts`.
fn account_preauth_key(name: &str, address: &str) -> Result<stellar_cli::xdr::ScVal, Error> {
    use stellar_cli::xdr;
    let address: xdr::ScAddress = address
        .parse()
        .map_err(|_| Error::InvalidAddress(address.to_string()))?;
    let canonical = name::canonicalize(name);
    Ok(xdr::ScVal::Vec(Some(
        vec![
            xdr::ScVal::String(xdr::ScString(canonical.as_str().try_into()?)),
            xdr::ScVal::Address(address),
        ]
        .try_into()?,
    )))
}

/// Stellar Address for G account for registry project
/// # Unsafe
/// It parse
pub fn stellar_address() -> stellar_strkey::ed25519::PublicKey {
    unsafe {
        "GAMPJROHOAW662FINQ4XQOY2ULX5IEGYXCI4SMZYE75EHQBR6PSTJG3M"
            .parse()
            .unwrap_unchecked()
    }
}

pub fn contract_id(network_passphrase: &str, salt: &str) -> stellar_strkey::Contract {
    PreHashContractID::new(stellar_address(), salt)
        .id(&stellar_build::Network::from_passphrase(network_passphrase).unwrap())
}

pub fn verified_contract_id(network_passphrase: &str) -> stellar_strkey::Contract {
    contract_id(network_passphrase, include_str!("../.salt").trim())
}

#[cfg(test)]
mod generate_id {
    use expect_test::{Expect, expect};
    use stellar_cli::config::network::passphrase::*;

    /// Run with `UPDATE_EXPECT=1 cargo test` to regenerate the expected contract
    /// IDs in-place after bumping the registry version or src/registry/.salt
    fn check(passphrase: &str, expected: &Expect) {
        expected.assert_eq(&super::verified_contract_id(passphrase).to_string());
    }

    #[test]
    fn futurenet() {
        check(
            FUTURENET,
            &expect!["CC6YGVN57L5XC4HDZ7AIUJSURGWGC2EEQDKC6NK4DZ6LUHTZY5WXOTLX"],
        );
    }

    #[test]
    fn testnet() {
        check(
            TESTNET,
            &expect!["CAAXJETKPYAATU4HVVQUTE2FFBULNFGZNEOC3MS635U5K3GZLAY2HI4M"],
        );
    }

    #[test]
    fn mainnet() {
        check(
            MAINNET,
            &expect!["CDU4M3LDIOUJJ5F3YXKJ4EJEP5VPRPG6N2LJ5HOQIMN7MNGL3NS3EGUY"],
        );
    }

    #[test]
    fn local() {
        check(
            LOCAL,
            &expect!["CA55VGAFPIZHOY2X26KANRJYFBWPEXGNLIEHR7Q5TR2576HKHOFPLBTX"],
        );
    }
}

// These pin the storage layout the account and flag reads depend on. If the
// registry contract changes it, update both sides together.
#[cfg(test)]
mod storage_keys {
    use expect_test::expect;
    use stellar_cli::xdr::{self, Limits, WriteXdr};

    const G: &str = "GAMPJROHOAW662FINQ4XQOY2ULX5IEGYXCI4SMZYE75EHQBR6PSTJG3M";

    fn b64(val: &xdr::ScVal) -> String {
        val.to_xdr_base64(Limits::none()).unwrap()
    }

    #[test]
    fn entry_keys_are_prefixed_canonical_names() {
        let key = super::entry_key("AC", "My_Account").unwrap();
        assert_eq!(key, super::entry_key("AC", "my-account").unwrap());
        expect!["AAAAEAAAAAEAAAACAAAADwAAAAJBQwAAAAAADgAAAApteS1hY2NvdW50AAA="]
            .assert_eq(&b64(&key));
        expect!["AAAAEAAAAAEAAAACAAAADwAAAAJDUgAAAAAADgAAAApteS1hY2NvdW50AAA="]
            .assert_eq(&b64(&super::entry_key("CR", "my-account").unwrap()));
    }

    #[test]
    fn account_preauth_key_is_name_then_address() {
        let key = super::account_preauth_key("My_Account", G).unwrap();
        expect!["AAAAEAAAAAEAAAACAAAADgAAAApteS1hY2NvdW50AAAAAAASAAAAAAAAAAAY9MXHcC3vaKhsOXg7GqLv1BDYuJHJMzgn+kPAMfPlNA=="].assert_eq(&b64(&key));
        assert!(super::account_preauth_key("x", "not-an-address").is_err());
    }

    #[test]
    fn flag_is_the_entry_length() {
        let addr = xdr::ScVal::Address(G.parse().unwrap());
        let entry = |n: usize| {
            let mut v = vec![addr.clone(), addr.clone()];
            if n == 3 {
                v.push(xdr::ScVal::Void);
            }
            xdr::ScVal::Vec(Some(v.try_into().unwrap()))
        };
        assert!(!super::is_flagged_entry(&entry(2)));
        assert!(super::is_flagged_entry(&entry(3)));
    }
}
