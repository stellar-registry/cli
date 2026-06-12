# Project Title

Verified Stellar Sources

# Submission title
> Make a unique title that's different from your project name and reflects what you're asking funding for

Source Verification Network & Hub

# One Sentence Description
> Use less than 130 characters to describe the products/services you`re planning to build in your submission, how it works, and your target audience. This is your opportunity to capture the attention of your reader and gain buy-in! A frequently used format is: “Develops/Offers/Gives/etc. _(a defined offering)_ to help/support _(a defined audience)_ _(solve a problem)_ with _(secret sauce)_”. Read more about this format and see examples in this blog post.

A network of Source Verification services, led by Runtime Verification. An hub for verification data, built on Stellar Registry.

# Code URL
> Enter the URL of the relevant github page.	

https://github.com/stellar-registry

# Video URL
> This is your elevator pitch: keep it short (<3 min), powerful, and clearly demonstrate the project's features and functionality. Upload your demo video on Youtube or Vimeo with a 16:9 aspect ratio (ideally 1920px by 1080px).


# Soroban

Yes


# Product and Services
> Briefly describe the to be added / improved products and services by this submission. Keep it succinct, and for each feature add how Stellar is used and how the improvements will impact your project.

We will build a network of verification services and the on-chain, public-infrastructure hub to integrate them all and surface their information. 

1. Source-verification services: following the example already provided in the [SEP precursor discussion](https://github.com/orgs/stellar/discussions/1923), these services will monitor wasms uploaded to Stellar that contain the `(source code, build image)` pair given by the SEP (the full list of possible `contractmetav0` items, which can be combined in a few valid ways, is: `bldimg` / `bldopt` / `source_repo` / `source_rev` / `tarball_url` / `tarball_sha256`). For each such wasm, these services will:  
   1. Check if the `bldimg` is from a trusted source. For now, this means it must refer to a [SDF-maintained `stellar-cli` container](https://hub.docker.com/r/stellar/stellar-cli).  
   2. Check that the source code is publicly available and well-formed (meaning there is a `source_rev` that matches the one given). (This proposal only covers open source repositories for now; see the Future Work section outlining a closed-source-with-auditing alternative as outlined in [Chad’s Item \#4 from the SEP precursor discussion](https://github.com/orgs/stellar/discussions/1923#discussioncomment-16882699).)  
   3. If both of the above are true, pull that container, pull that source code, and run `stellar contract verify` for that source code within that container.  
   4. Check if the result of `verify` confirms that the given `(source code, build image)` pair do, indeed, result in the `wasm_hash` of the uploaded wasm.
   5. Submit the result of this check to Stellar Registry (see below).
2. Stellar Registry data hub: Stellar Registry already allows adding names and semantic versions to Stellar smart contracts, turning the inscrutable trail of wasm hash uploads into a meaningful (and instrumentable) narrative. Once `(source code, build image)` information is added to the wasm metadata, Registry will also expose this data via its contract calls and UI.

   Crucially, this information will initially be shown as *unverified*. We will add a `verify_source` contract endpoint, gated to an allow-list of known Source-Verification services (item #1 above). Rather than showing a simple checkmark if a source-verification service confirms a `source code + build image = wasm hash` match, Registry will expose *which* verifiers said *what*. It will surface disagreements, allowing consumers to make their own decision about whether to trust a wasm that is source-verified by some entities but which fails source-verification for others.

   Additionally, for wasms that have already been uploaded to Stellar which lack the new `(source code, build image)` data in their `contractmetav0`, Registry will allow adding these fields via the Registry contract. It will then emit an event which can be monitored by Source Verification Services, in addition to wasm uploads, so that their source verification process can run based on both on-chain events.

This is a joint submission by **The Aha Company** and **Runtime Verification**. The Aha Company will build the Stellar Registry data integration point. Runtime Verification will provide an initial source-verification service. All aspects, from both Aha and RV, will be open source and well-documented, providing the whole ecosystem a solid foundation for 1\. retrieving source-verification information from Stellar Registry and 2\. implementing their own source-verification system, following RV’s example. 


# Traction Evidence
> Provide evidence of prior traction or validation. Include users, community size, partnerships, KPIs (TVL...), beta testers to demonstrate adoption, market interest, and project credibility.

## The Aha Company

A Stellar-focused engineering firm with a strong delivery record: major contributions to the Stellar CLI, institutional work bringing Société Générale-Forge's EURCV stablecoin onto Stellar rails, and multiple SCF-funded public goods including Moonlight and Scaffold Stellar. We support ecosystem governance and developer education (largely through hackathons).

This project extends infrastructure we already operate, Stellar Registry, which includes:

* [`stellar-registry-cli`](https://crates.io/crates/stellar-registry-cli) published on crates.io (\~6,400+ all-time downloads).  
* Live registry browser at [https://stellar.rgstry.xyz](https://stellar.rgstry.xyz); registry contract on testnet, mainnet rollout underway.  
* Active in the SEP-55 / reproducible-builds discussions (\#1573, \#1923), where this registry effort is already flagged.

## Runtime Verification

A leading formal-verification and smart-contract security firm. Founded in 2010 by Grigore Roșu, creator of the K framework, it has since 2018 delivered formal verification and security services to major blockchain foundations and protocols including the Ethereum Foundation, Stellar, EigenLayer, Optimism, Uniswap, Lido, Gnosis, and Morpho. RV maintains widely-used open-source verification tooling such as Kontrol and Simbolik, and its formal-verification engine now powers Immunefi's Magnus security platform. RV also has extensive experience in the Stellar and Soroban ecosystem, having provided high-quality audits to DeFi protocols deploying on Stellar for over 2 years and developed tooling for Soroban like [Komet](https://komet.runtimeverification.com/) for fuzz testing and formal verification. RV will operate an independent verifier node in the network and provide audit-grade trust signals.

## Ethan Frey ([@ethanfrey](https://github.com/ethanfrey))

Creator of CosmWasm, the WebAssembly smart-contract platform that directly inspired Soroban's architecture — Wasm runtime, Rust SDK, host-function model, and contract metadata sections all overlap. Through Confio, the company he founded, and building on earlier work as a contributor to the Cosmos SDK, CosmWasm grew into the dominant smart-contract engine in the Cosmos ecosystem, deployed across 30+ independent app-chains (Osmosis, Neutron, Sei, Injective, Archway, and others) and securing billions in on-chain value. He brings direct prior art for this RFP's hardest problem: he designed and shipped [CosmWasm/optimizer](https://github.com/CosmWasm/optimizer), the deterministic, toolchain-pinned Wasm build image that essentially every CosmWasm contract verification has run through for years and the proven blueprint for this project's Soroban build pipeline, along with the hard-won lessons (`--locked` enforcement, single-architecture determinism) now feeding directly into SEP-58; and he built [cosmwasm-verify](https://github.com/CosmWasm/cosmwasm-verify), a source-verification proof-of-concept (git repo + commit → deployed Wasm hash) that is essentially the centralized version of what this proposal decentralizes for Soroban. As an advisor to [Warp Drive](https://www.warp-drive.xyz), the Stellar port of [WAVS](https://wavs.xyz), he also brings the EigenLayer-style restaked operator network that provides the multi-operator, cryptoeconomically secured attestation layer this proposal depends on. He is actively shaping the live SEP-58 / reproducible-builds design (Stellar discussions [#1923](https://github.com/orgs/stellar/discussions/1923)) and working to standardize architecture deicisions via the SEP-process, like [Stellar Protocol PR #1951](https://github.com/stellar/stellar-protocol/pull/1951), so this verifier composes with the emerging standard rather than working around it, with all work MIT-licensed and built in the open.


# Technical Architecture
> Add an accessible link to your technical architecture of the to be added / improved product/services.
>
> The technical architecture should go into the specifics on the Stellar integration, demonstrating you have sufficient insight to start building immediately.
>
> We invite you to explore the various tools (CLI, SDK, etc.) provided by LOAM to facilitate your integration on Stellar: https://github.com/loambuild/loam?tab=readme-ov-file
>
> Here are examples
> - https://docs.google.com/document/d/1tXGALHBpMhbATfSnPKARBqctoyGoRPuge4Ukiv8j_Hc/edit?tab=t.0
> - https://github.com/FrankSzendzielarz/SorobanRPCSDK
> => Good one - https://drive.google.com/file/d/167c6KVuyvRo7haInUahDfeMRxkO8gya3/view"	

[technical-architecture-highlevel.md](./technical-architecture-highlevel.md)

# SCF Build Tranche Deliverables

> Depending on the scope of your project, importance to the ecosystem, and team, you can request up to $150K in XLM for ~6 months to cover costs directly related to development of the product itself with the final goal being to launch on mainnet. Take a close look at the to determine your total budget request.
>
> You should divide your deliverables into 3 main milestones: Minimum Viable Product (MVP), Testnet, and Mainnet.
>
> Your award will be distributed in 3 equivalent tranches. The first upon approval of the submission, the second upon deliverable review of your MVP, and the third when you launch on Mainnet. The testnet tranche will not unlock an XLM award, but will provide you with access to the Stellar LaunchKit where you can apply for credits for audits and infrastructure offers. For each tranche, include estimated dates of completion, and a budget breakdown for each deliverable.
>
> Common issues:
> 1. SCF funding may not be used for marketing or promotion. Do not include these items in your budget.
>
> 2. Audit credits are provided as part of the testnet tranche completion. Audit costs should not be included in your budget.


## Tranche 1 - MVP

[Deliverable 1]

- Brief description
- How to measure completion
- Estimated date of completion
- Budget

[Deliverable 2]

etc.


## Tranche 2 - Testnet: 

[Deliverable 1]

- Brief description
- How to measure completion
- Estimated date of completion
- Budget

[Deliverable 2]

etc.


## Tranche 3 - Mainnet: 

[Deliverable 1]

- Brief description
- How to measure completion
- Estimated date of completion
- Budget

[Deliverable 2]

etc.


# Budget Total
Please note: submission with budgets amounts exceeding the maximum will be denied without review. Find budget guidelines for each award in the SCF Handbook.




