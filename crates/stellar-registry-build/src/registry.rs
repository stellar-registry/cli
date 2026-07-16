use stellar_cli::config;

use crate::{
    Error,
    contract::{Contract, PreHashContractID},
    name,
};

pub struct Registry(Contract);

impl name::Prefixed {
    /// Resolve the (sub)registry this name's channel points at.
    pub async fn registry(&self, config: &config::Args) -> Result<Registry, Error> {
        Registry::from_named_registry(config, self).await
    }
}

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
        use stellar_cli::xdr;
        let canonical = name::canonicalize(name);
        let key = xdr::ScVal::Vec(Some(
            vec![
                xdr::ScVal::Symbol(xdr::ScSymbol("CR".try_into()?)),
                xdr::ScVal::String(xdr::ScString(canonical.as_str().try_into()?)),
            ]
            .try_into()?,
        ));
        let ledger_key = xdr::LedgerKey::ContractData(xdr::LedgerKeyContractData {
            contract: self.0.sc_address(),
            key,
            durability: xdr::ContractDataDurability::Persistent,
        });
        let entries = self
            .0
            .rpc_client()?
            .get_full_ledger_entries(&[ledger_key])
            .await?
            .entries;
        Ok(entries.into_iter().any(|e| match e.val {
            xdr::LedgerEntryData::ContractData(cd) => {
                matches!(cd.val, xdr::ScVal::Vec(Some(v)) if v.len() == 3)
            }
            _ => false,
        }))
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
