# Contract Source Verification Service

This tracks our application for the [Contract Source Verification Service RFP](https://stellar.gitbook.io/scf-handbook/scf-awards/build-award/rfp-track#contract-source-verification-service). In case of link rot, the full text of the original RFP is pasted below:

### Contract Source Verification Service

*Added: Q2 2026*

#### 1. Scope of Work

Design and deliver a public, hosted contract source verification service for Soroban smart contracts. The service consumes the [SEP-58](https://github.com/stellar/stellar-protocol/pull/1933) metadata vocabulary (bldimg, bldopt, source\_repo, source\_rev, tarball\_url, tarball\_sha256), rebuilds submitted source in a defined environment, compares the resulting Wasm to the deployed Wasm, and exposes the result to downstream consumers (explorers, wallets, the CLI). The service must cover both public source (git repo plus commit) and private source (content-addressed via `tarball_sha256` alone, suitable for auditor-mediated rebuilds).

This service sits downstream of an already-shipping foundation. SDF is publishing the trusted stellar-cli-docker image plus "how to build" and "how to verify" guides on a shorter timeline. Asset issuers, contract developers, and planned verifiers (OrbitLens, Aha Labs, 57B) use those directly in the near term. The hosted service this RFP funds aggregates verifications across the ecosystem and serves results to downstream tools.

Deliverables span:

* **Audit readiness:** threat model, security review, and audit fixes prior to production handoff.
* **Offchain service components:** a verification service that accepts source submissions (tarballs or equivalent), rebuilds the Wasm using an SDF-allowlisted trusted build image, and links the resulting Wasm digest to the source artifact.
* **Public API:** a stable, free API for querying verification status by contract ID or Wasm hash. Returns SEP-58 fields plus verification status and timestamps.
* **Integration examples:** reference integrations for explorers, wallets, and other downstream tools.
* **Developer-facing submission flow:** a CLI or web interface so contract authors can submit their own deployments.
* **Documentation:** contributor-facing docs for submitters and integrator-facing docs for consumers
* **Audit readiness:** threat model, security review, and audit fixes prior to production handoff.

**Explicitly out of scope:**

* **Display-layer policy decisions by individual verifiers** (e.g., whether to surface verifications for contracts without publicly retrievable source). The service may expose auditor signals or off-chain attestations if available, but is not required to define their semantics.
* **Authoring SEP-58, SEP-55.** Coordination is expected; authorship is not.
* **Producing the official trusted stellar-cli Docker images.** SDF maintains these via stellar-cli-docker. Vendors consume the SDF-published images.
* **Updates to SDF-owned tooling** (stellar-cli, Stellar Lab) beyond exposing the APIs those tools consume. SDF builds those integrations in-house.
* **Explorer UI work on any specific third-party explorer** (Stellar Expert, StellarChain), though the service must expose what they need.
* **Fully deterministic Rust compilation as a research effort.** Use the best available reproducibility today, anchored on the SDF-allowlisted trusted images.

#### 2. Background & Context

[SEP-58](https://github.com/stellar/stellar-protocol/pull/1933) (Draft, in active review since 2026-05-15) defines the metadata a Soroban contract embeds, or surfaces off-chain, so any verifier with the source can rebuild the Wasm and confirm the bytes match. SEP-58 is a vocabulary. It leaves the service implementation, build infrastructure, and explorer integrations to ecosystem teams. This RFP funds the service layer.

SEP-58 complements [SEP-55](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0055.md), which uses signed CI attestations to bind a workflow run to a commit and a Wasm. SEP-55 asks "did a trusted CI compile this Wasm?". SEP-58 asks "does this source produce this Wasm?". A contract can carry meta supporting both. Verifiers pick whichever path fits their threat model.

Soroban contracts deploy as opaque Wasm bytes. Today, an explorer can display source code linked to a contract, but the connection between that source and the deployed bytes is not protocol-level provable. SEP-58 provides the standard inputs. The remaining work is a service that performs rebuilds and serves results to downstream tools, so each tool doesn't run its own rebuild infrastructure.

SEP-58 is mode-agnostic about source retrieval. Public repos use source\_repo + source\_rev. Hosted tarballs use tarball\_url + tarball\_sha256. Closed-source builds use tarball\_sha256 alone, committing to specific source bytes without naming a retrieval channel (suitable when only an auditor with access can rebuild). What differs between public and private source isn't the spec; it's who rebuilds and which verifiers display the result. Some verifiers (e.g., Stellar Expert) will not auto-verify contracts without publicly retrievable source. The service must support all SEP-58 source modes; downstream display decisions are out of scope.

**Ecosystem context:**

* **Complementary CLI work:** stellar-cli [PR #2585](https://github.com/stellar/stellar-cli/pull/2585) adds `stellar contract build --verifiable`, which runs the build inside a digest-pinned Docker container and stamps SEP-58 metadata into the Wasm. [PR #2586](https://github.com/stellar/stellar-cli/pull/2586) adds `stellar contract verify`, which reads the metadata, re-runs the recorded build, and byte-compares the result. The CLI verifies locally with no service dependency. This RFP funds the shared layer aggregating results across verifiers.
* **Trusted build images:** SDF maintains stellar-cli-docker, which publishes attested Docker images for use as bldimg values. The publish pipeline ([PR #3](https://github.com/stellar/stellar-cli-docker/pull/3)) targets a first published image in late May 2026. It covers public Dockerfiles, SLSA build provenance, SBOMs, pinned base images, and a defined release cadence.
* **Multi-dimensional trust:** reproducibility alone is not faithfulness to source. A hostile image can deterministically rewrite bytes and still pass byte-comparison. The service should treat image trust as a signal (arbitrary deployer image, publicly auditable image, SDF-maintained trusted image), not a binary.
* **Existing in-house prototype:** [stellar-experimental/contract-verifications](https://github.com/stellar-experimental/contract-verifications) is a starting point but is not production-ready.
* **Internal boundary:** SDF builds the CLI, trusted images, and Stellar Lab integrations. The hosted public verification service is a fit for external funding.

Other ecosystems have solved this via [Sourcify](https://sourcify.dev) (EVM) and [solana-verify](https://solana.com/docs/programs/verified-builds). Stellar needs an equivalent, either adapted from existing tooling or purpose-built for Soroban. Respondents may propose either approach.

#### 3. Requirements

**Functional requirements:**

* Accept source code submissions (tarball or equivalent) tied to a target Wasm hash.
* Rebuild the submitted source using an **SDF-allowlisted trusted build image** referenced via SEP-58 bldimg. Vendor describes its image allowlist policy in the proposal.
* Expose a free, public API for querying verification status by contract ID or Wasm hash. Response includes at minimum: verification status, SEP-58 fields recorded (bldimg, bldopt, source\_repo, source\_rev, tarball\_url, tarball\_sha256 as applicable), linked source artifact, build metadata, image trust signal, and verification timestamp.
* Provide a shared verification result layer that explorers, Lab, wallets, and other consumers can query without performing their own rebuilds. Verification runs once and the result is available via API. Hard requirement: solutions that force every consumer to rebuild, or that rely on a single hardcoded verifier, do not meet the bar.
* Support **multi-verifier architecture.** Each verifier reports its result against the deployed Wasm hash. Disagreement surfaces as a per-verifier signal (e.g., \[√] Verified by X, \[!] Mismatching verification by Y), not a single "correct" status.
* Support mainnet and testnet.
* Handle contracts deployed before service launch (**retroactive verification for non-upgradable contracts is a priority requirement,** since many existing deployments cannot be redeployed).
* Source retrieval must not hardcode HTTPS or GitHub. The API and rebuild flow accept all SEP-58 source identifier modes: public repo (`source_repo` + `source_rev`), hosted tarball (`tarball_url` + `tarball_sha256`), and content-addressed (`tarball_sha256` alone). **IPFS retrieval is a first-tier channel alongside HTTPS.** Vendors may propose which modes their service handles directly versus surfaces from off-chain sources; the API must distinguish modes in responses.
* Provide a CLI or developer-facing submission flow. API shaped to integrate with [stellar-cli](https://github.com/stellar/stellar-cli).
* Expose verification metadata in a form consumable by explorers (Stellar Expert, StellarChain, [Stellar Lab](https://github.com/stellar/laboratory)) and other downstream tools.
* **Coexist with SEP-55 attestations and SEP-58 rebuild verification as distinct trust levels.** The API distinguishes them.

Conform to the forthcoming verifier-API SEP (currently unauthored, expected to follow SEP-58). Vendor commits to align the API once that spec lands.

**Non-functional requirements:**

* **Explain your approach** in the [SEP-58 discussion thread](https://github.com/orgs/stellar/discussions/1923) before or during proposal submission.
* **Security**: tarball storage must be tamper-evident; rebuild environment isolated from host; submitted code must not exfiltrate secrets or affect other submissions; write endpoints must have abuse and DoS protections.
* **Audit**: third-party security audit required before production launch. Scope: rebuild environment, tarball integrity, API authentication, image allowlist policy. Auditor coordinated by SDF via the audit bank.
* **UX**: a contract developer should verify a deployed contract in under 15 minutes from reading the docs to seeing the verification appear.
* **Decentralization of verification**: architecture must allow multiple independent verifier instances to publish results. Proposals describe how disagreement surfaces and how consumers pick a trusted set.
* **Performance:** verification requests complete or return queued status within 5 minutes for standard contract sizes.
* **Availability:** public query API targets 99%+ uptime.
* **Storage and retention:** vendors propose tarball and rebuild-artifact retention policy. Egress cost ownership made explicit.
* **Operational ownership post-grant:** hosting, on-call rotation, and funding tail addressed in the proposal.
* **Compliance:** operable as a public good without KYC or gated access. No user data collection beyond what's necessary for abuse prevention.
* **Openness:** codebase open-source and self-hostable. Production deployment should allow community operation over time.<br>

**Interfaces SDF will provide before contract execution:**

* The CLI to Service interaction contract (being resolved internally). Vendors should support either a service-mediated submission flow or a decentralized on-chain result discovery model; SDF will name the shape before the design phase.
* The stellar-cli-docker image allowlist endpoint and trust criteria for adding image sources.
* Any SEP-58 amendments landing before the vendor's design phase (e.g., the home\_domain field amendment under discussion).

#### 4. Evaluation Criteria

* **Technical capability:** demonstrated experience building reproducible build pipelines and verification infrastructure. Familiarity with Sourcify, solana-verify, or equivalent is strong evidence.
* **SEP-58 alignment:** proposal maps to the SEP-58 vocabulary and consumes SDF-published trusted images via bldimg.
* **Display-layer design:** how downstream tools (explorers, Lab, wallets) consume verification status without running their own rebuilds, including multi-verifier divergence UX.
* **Decentralization design:** concrete architecture for running multiple independent verifier instances.
* **Image trust policy:** allowlist policy and handling of image source eviction.
* **Relevant experience:** prior work on Sourcify or equivalent services, or deep Soroban / Rust toolchain experience.
* **Security & audit history:** track record of shipping audited infrastructure; threat modeling in the proposal.
* **Ecosystem alignment:** willingness to coordinate with explorer teams (Stellar Expert, StellarChain), SDF tooling teams (stellar-cli, Stellar Lab), and ongoing SEP discussions ([SEP-58](https://github.com/orgs/stellar/discussions/1923), the forthcoming verifier-API SEP, SEP-55 evolution).
* **Ability to deliver within required timeline:** realistic milestone plan.
* **Coherent integration plan:** how explorers and other consumers adopt the service, including API stability guarantees and the path to conformance with the forthcoming verifier-API SEP.

#### 5. Expected Deliverables

* Verification service codebase, open-source and self-hostable.
* Public deployment for mainnet and testnet.
* Stable public API with documented schema and versioning policy.
* SDK or client library for querying verification status, designed to conform with the forthcoming verifier-API SEP once authored.
* Contract developer CLI or submission interface.
* Image allowlist policy documented, with a published mechanism for adding new image sources.
* Integration documentation for explorers and downstream tools.
* User documentation for contract developers, contributed to Stellar Developer Docs.
* Test suite covering verification logic, API, rebuild environment, and image-trust signals.
* Security audit report and resolved findings (through Audit Bank).
* Production-ready service with operational runbook, monitoring, and on-call coverage.
* At least one reference integration, ideally [Stellar Lab](https://github.com/stellar/laboratory) or a cooperating verifier (OrbitLens / Stellar Expert, Aha Labs / rgstry.xyz, or 57B).
