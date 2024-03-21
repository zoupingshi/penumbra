use anyhow::Context;
use bytes::Bytes;
use penumbra_funding::FundingParameters;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::params::GovernanceParameters;
use penumbra_community_pool::params::CommunityPoolParameters;
use penumbra_dex::DexParameters;
use penumbra_distributions::params::DistributionsParameters;
use penumbra_fee::params::FeeParameters;
use penumbra_ibc::params::IBCParameters;
use penumbra_proto::{penumbra::core::component::governance::v1 as pb, DomainType};
use penumbra_sct::params::SctParameters;
use penumbra_shielded_pool::params::ShieldedPoolParameters;
use penumbra_stake::params::StakeParameters;

/// A governance proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::Proposal", into = "pb::Proposal")]
pub struct Proposal {
    /// The ID number of the proposal.
    pub id: u64,

    /// A short title describing the intent of the proposal.
    pub title: String,

    /// A natural-language description of the effect of the proposal and its justification.
    pub description: String,

    /// The specific kind and attributes of the proposal.
    pub payload: ProposalPayload,
}

/// The protobuf type URL for a transaction plan.
pub const TRANSACTION_PLAN_TYPE_URL: &str = "/penumbra.core.transaction.v1.TransactionPlan";

impl From<Proposal> for pb::Proposal {
    fn from(inner: Proposal) -> pb::Proposal {
        let mut proposal = pb::Proposal {
            id: inner.id,
            title: inner.title,
            description: inner.description,
            ..Default::default() // We're about to fill in precisely one of the fields for the payload
        };
        use pb::proposal::Payload;
        let payload = match inner.payload {
            ProposalPayload::Signaling { commit } => {
                Some(Payload::Signaling(pb::proposal::Signaling {
                    commit: if let Some(c) = commit {
                        c
                    } else {
                        String::default()
                    },
                }))
            }
            ProposalPayload::Emergency { halt_chain } => {
                Some(Payload::Emergency(pb::proposal::Emergency { halt_chain }))
            }
            ProposalPayload::ParameterChange { old, new } => {
                Some(Payload::ParameterChange(pb::proposal::ParameterChange {
                    old_parameters: Some((*old).into()),
                    new_parameters: Some((*new).into()),
                }))
            }
            ProposalPayload::CommunityPoolSpend { transaction_plan } => Some(
                Payload::CommunityPoolSpend(pb::proposal::CommunityPoolSpend {
                    transaction_plan: Some(pbjson_types::Any {
                        type_url: TRANSACTION_PLAN_TYPE_URL.to_owned(),
                        value: transaction_plan.into(),
                    }),
                }),
            ),
            ProposalPayload::UpgradePlan { height } => {
                Some(Payload::UpgradePlan(pb::proposal::UpgradePlan { height }))
            }
            ProposalPayload::FreezeIbcClient { client_id } => {
                Some(Payload::FreezeIbcClient(pb::proposal::FreezeIbcClient {
                    client_id: client_id.into(),
                }))
            }
            ProposalPayload::UnfreezeIbcClient { client_id } => Some(Payload::UnfreezeIbcClient(
                pb::proposal::UnfreezeIbcClient {
                    client_id: client_id.into(),
                },
            )),
        };
        proposal.payload = payload;
        proposal
    }
}

impl TryFrom<pb::Proposal> for Proposal {
    type Error = anyhow::Error;

    fn try_from(inner: pb::Proposal) -> Result<Proposal, Self::Error> {
        use pb::proposal::Payload;
        Ok(Proposal {
            id: inner.id,
            title: inner.title,
            description: inner.description,
            payload: match inner
                .payload
                .ok_or_else(|| anyhow::anyhow!("missing proposal payload"))?
            {
                Payload::Signaling(signaling) => ProposalPayload::Signaling {
                    commit: if signaling.commit.is_empty() {
                        None
                    } else {
                        Some(signaling.commit)
                    },
                },
                Payload::Emergency(emergency) => ProposalPayload::Emergency {
                    halt_chain: emergency.halt_chain,
                },
                Payload::ParameterChange(parameter_change) => ProposalPayload::ParameterChange {
                    old: Box::new(
                        parameter_change
                            .old_parameters
                            .ok_or_else(|| anyhow::anyhow!("missing old parameters"))?
                            .try_into()?,
                    ),
                    new: Box::new(
                        parameter_change
                            .new_parameters
                            .ok_or_else(|| anyhow::anyhow!("missing new parameters"))?
                            .try_into()?,
                    ),
                },
                Payload::CommunityPoolSpend(community_pool_spend) => {
                    ProposalPayload::CommunityPoolSpend {
                        transaction_plan: {
                            let transaction_plan = community_pool_spend
                                .transaction_plan
                                .ok_or_else(|| anyhow::anyhow!("missing transaction plan"))?;
                            if transaction_plan.type_url != TRANSACTION_PLAN_TYPE_URL {
                                anyhow::bail!(
                                    "unknown transaction plan type url: {}",
                                    transaction_plan.type_url
                                );
                            }
                            transaction_plan.value.to_vec()
                        },
                    }
                }
                Payload::UpgradePlan(upgrade_plan) => ProposalPayload::UpgradePlan {
                    height: upgrade_plan.height,
                },
                Payload::FreezeIbcClient(freeze_ibc_client) => ProposalPayload::FreezeIbcClient {
                    client_id: freeze_ibc_client.client_id,
                },
                Payload::UnfreezeIbcClient(unfreeze_ibc_client) => {
                    ProposalPayload::UnfreezeIbcClient {
                        client_id: unfreeze_ibc_client.client_id,
                    }
                }
            },
        })
    }
}

impl DomainType for Proposal {
    type Proto = pb::Proposal;
}

/// A human-readable TOML-serializable version of a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalToml {
    pub id: u64,
    pub title: String,
    pub description: String,
    #[serde(flatten)]
    pub payload: ProposalPayloadToml,
}

impl From<Proposal> for ProposalToml {
    fn from(proposal: Proposal) -> ProposalToml {
        ProposalToml {
            id: proposal.id,
            title: proposal.title,
            description: proposal.description,
            payload: proposal.payload.into(),
        }
    }
}

impl TryFrom<ProposalToml> for Proposal {
    type Error = anyhow::Error;

    fn try_from(proposal: ProposalToml) -> Result<Proposal, Self::Error> {
        Ok(Proposal {
            id: proposal.id,
            title: proposal.title,
            description: proposal.description,
            payload: proposal.payload.try_into()?,
        })
    }
}

/// The specific kind of a proposal.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
pub enum ProposalKind {
    /// A signaling proposal.
    #[cfg_attr(feature = "clap", clap(display_order = 100))]
    Signaling,
    /// An emergency proposal.
    #[cfg_attr(feature = "clap", clap(display_order = 200))]
    Emergency,
    /// A parameter change proposal.
    #[cfg_attr(feature = "clap", clap(display_order = 300))]
    ParameterChange,
    /// A Community Pool spend proposal.
    #[cfg_attr(feature = "clap", clap(display_order = 400))]
    CommunityPoolSpend,
    /// An upgrade proposal.
    #[cfg_attr(feature = "clap", clap(display_order = 500))]
    UpgradePlan,
    /// A proposal to freeze an IBC client.
    #[cfg_attr(feature = "clap", clap(display_order = 600))]
    FreezeIbcClient,
    /// A proposal to unfreeze an IBC client.
    #[cfg_attr(feature = "clap", clap(display_order = 700))]
    UnfreezeIbcClient,
}

impl FromStr for ProposalKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "signaling" => Ok(ProposalKind::Signaling),
            "emergency" => Ok(ProposalKind::Emergency),
            "parameter_change" => Ok(ProposalKind::ParameterChange),
            "community_pool_spend" => Ok(ProposalKind::CommunityPoolSpend),
            "upgrade_plan" => Ok(ProposalKind::UpgradePlan),
            _ => Err(anyhow::anyhow!("invalid proposal kind: {}", s)),
        }
    }
}

impl Proposal {
    /// Get the kind of a proposal.
    pub fn kind(&self) -> ProposalKind {
        match self.payload {
            ProposalPayload::Signaling { .. } => ProposalKind::Signaling,
            ProposalPayload::Emergency { .. } => ProposalKind::Emergency,
            ProposalPayload::ParameterChange { .. } => ProposalKind::ParameterChange,
            ProposalPayload::CommunityPoolSpend { .. } => ProposalKind::CommunityPoolSpend,
            ProposalPayload::UpgradePlan { .. } => ProposalKind::UpgradePlan,
            ProposalPayload::FreezeIbcClient { .. } => ProposalKind::FreezeIbcClient,
            ProposalPayload::UnfreezeIbcClient { .. } => ProposalKind::UnfreezeIbcClient,
        }
    }
}

/// The machine-interpretable body of a proposal.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProposalPayload {
    /// A signaling proposal is merely for coordination; it does not enact anything automatically by
    /// itself.
    Signaling {
        /// An optional commit hash for code that this proposal refers to.
        commit: Option<String>,
    },
    /// An emergency proposal is immediately passed when 2/3 of all validators approve it, without
    /// waiting for the voting period to conclude.
    Emergency {
        /// If `halt_chain == true`, then the chain will immediately halt when the proposal is
        /// passed.
        halt_chain: bool,
    },
    /// A parameter change proposal describes a replacement of the app parameters, which should
    /// take effect when the proposal is passed.
    ParameterChange {
        /// The old app parameters to be replaced.
        ///
        /// Even if the proposal passes, the update will not be applied if the app parameters have
        /// changed *at all* from these app parameters. Usually, this should be set to the current
        /// app parameters at time of proposal.
        old: Box<ChangedAppParameters>,
        /// The new app parameters to be set.
        ///
        /// The *entire* app parameters will be replaced with these at the time the proposal is
        /// passed.
        new: Box<ChangedAppParameters>,
    },
    /// A Community Pool spend proposal describes proposed transaction(s) to be executed or cancelled at
    /// specific heights, with the spend authority of the Community Pool.
    CommunityPoolSpend {
        /// The transaction plan to be executed at the time the proposal is passed.
        ///
        /// This must be a transaction plan which can be executed by the Community Pool, which means it can't
        /// require any witness data or authorization signatures, but it may use the `CommunityPoolSpend`
        /// action.
        transaction_plan: Vec<u8>,
    },
    /// An upgrade plan proposal describes a planned upgrade to the chain. If ratified, the chain
    /// will halt at the specified height, trigger an epoch transition, and halt the chain.
    UpgradePlan { height: u64 },
    /// A proposal to freeze a specific IBC client.
    FreezeIbcClient {
        /// The identifier of the client to freeze.
        client_id: String,
    },
    /// A proposal to unfreeze a specific IBC client.
    UnfreezeIbcClient {
        /// The identifier of the client to unfreeze.
        client_id: String,
    },
}

/// A TOML-serializable version of `ProposalPayload`, meant for human consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalPayloadToml {
    Signaling {
        commit: Option<String>,
    },
    Emergency {
        halt_chain: bool,
    },
    ParameterChange {
        old: Box<ChangedAppParameters>,
        new: Box<ChangedAppParameters>,
    },
    CommunityPoolSpend {
        transaction: String,
    },
    UpgradePlan {
        height: u64,
    },
    FreezeIbcClient {
        client_id: String,
    },
    UnfreezeIbcClient {
        client_id: String,
    },
}

impl TryFrom<ProposalPayloadToml> for ProposalPayload {
    type Error = anyhow::Error;

    fn try_from(toml: ProposalPayloadToml) -> Result<Self, Self::Error> {
        Ok(match toml {
            ProposalPayloadToml::Signaling { commit } => ProposalPayload::Signaling { commit },
            ProposalPayloadToml::Emergency { halt_chain } => {
                ProposalPayload::Emergency { halt_chain }
            }
            ProposalPayloadToml::ParameterChange { old, new } => {
                ProposalPayload::ParameterChange { old, new }
            }
            ProposalPayloadToml::CommunityPoolSpend { transaction } => {
                ProposalPayload::CommunityPoolSpend {
                    transaction_plan: Bytes::from(
                        base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            transaction,
                        )
                        .context("couldn't decode transaction plan from base64")?,
                    )
                    .to_vec(),
                }
            }
            ProposalPayloadToml::UpgradePlan { height } => ProposalPayload::UpgradePlan { height },
            ProposalPayloadToml::FreezeIbcClient { client_id } => {
                ProposalPayload::FreezeIbcClient { client_id }
            }
            ProposalPayloadToml::UnfreezeIbcClient { client_id } => {
                ProposalPayload::UnfreezeIbcClient { client_id }
            }
        })
    }
}

impl From<ProposalPayload> for ProposalPayloadToml {
    fn from(payload: ProposalPayload) -> Self {
        match payload {
            ProposalPayload::Signaling { commit } => ProposalPayloadToml::Signaling { commit },
            ProposalPayload::Emergency { halt_chain } => {
                ProposalPayloadToml::Emergency { halt_chain }
            }
            ProposalPayload::ParameterChange { old, new } => {
                ProposalPayloadToml::ParameterChange { old, new }
            }
            ProposalPayload::CommunityPoolSpend { transaction_plan } => {
                ProposalPayloadToml::CommunityPoolSpend {
                    transaction: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        transaction_plan,
                    ),
                }
            }
            ProposalPayload::UpgradePlan { height } => ProposalPayloadToml::UpgradePlan { height },
            ProposalPayload::FreezeIbcClient { client_id } => {
                ProposalPayloadToml::FreezeIbcClient { client_id }
            }
            ProposalPayload::UnfreezeIbcClient { client_id } => {
                ProposalPayloadToml::UnfreezeIbcClient { client_id }
            }
        }
    }
}

impl ProposalPayload {
    pub fn is_signaling(&self) -> bool {
        matches!(self, ProposalPayload::Signaling { .. })
    }

    pub fn is_emergency(&self) -> bool {
        matches!(self, ProposalPayload::Emergency { .. })
    }

    pub fn is_ibc_freeze(&self) -> bool {
        matches!(self, ProposalPayload::FreezeIbcClient { .. })
            || matches!(self, ProposalPayload::UnfreezeIbcClient { .. })
    }

    pub fn is_parameter_change(&self) -> bool {
        matches!(self, ProposalPayload::ParameterChange { .. })
    }

    pub fn is_community_pool_spend(&self) -> bool {
        matches!(self, ProposalPayload::CommunityPoolSpend { .. })
    }
}

/// Indicates which app parameters have changed during the
/// current block.
///
/// Note: must be kept in sync with
/// `penumbra_app::params::AppParameters`.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    try_from = "pb::ChangedAppParameters",
    into = "pb::ChangedAppParameters"
)]
pub struct ChangedAppParameters {
    pub community_pool_params: Option<CommunityPoolParameters>,
    pub distributions_params: Option<DistributionsParameters>,
    pub ibc_params: Option<IBCParameters>,
    pub fee_params: Option<FeeParameters>,
    pub funding_params: Option<FundingParameters>,
    pub governance_params: Option<GovernanceParameters>,
    pub sct_params: Option<SctParameters>,
    pub shielded_pool_params: Option<ShieldedPoolParameters>,
    pub stake_params: Option<StakeParameters>,
    pub dex_params: Option<DexParameters>,
}

impl DomainType for ChangedAppParameters {
    type Proto = pb::ChangedAppParameters;
}

impl TryFrom<pb::ChangedAppParameters> for ChangedAppParameters {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ChangedAppParameters) -> anyhow::Result<Self> {
        Ok(ChangedAppParameters {
            community_pool_params: msg
                .community_pool_params
                .map(TryInto::try_into)
                .transpose()?,
            distributions_params: msg
                .distributions_params
                .map(TryInto::try_into)
                .transpose()?,
            fee_params: msg.fee_params.map(TryInto::try_into).transpose()?,
            funding_params: msg.funding_params.map(TryInto::try_into).transpose()?,
            governance_params: msg.governance_params.map(TryInto::try_into).transpose()?,
            ibc_params: msg.ibc_params.map(TryInto::try_into).transpose()?,
            sct_params: msg.sct_params.map(TryInto::try_into).transpose()?,
            shielded_pool_params: msg
                .shielded_pool_params
                .map(TryInto::try_into)
                .transpose()?,
            stake_params: msg.stake_params.map(TryInto::try_into).transpose()?,
            dex_params: msg.dex_params.map(TryInto::try_into).transpose()?,
        })
    }
}

impl From<ChangedAppParameters> for pb::ChangedAppParameters {
    fn from(params: ChangedAppParameters) -> Self {
        pb::ChangedAppParameters {
            community_pool_params: params.community_pool_params.map(Into::into),
            distributions_params: params.distributions_params.map(Into::into),
            fee_params: params.fee_params.map(Into::into),
            funding_params: params.funding_params.map(Into::into),
            governance_params: params.governance_params.map(Into::into),
            ibc_params: params.ibc_params.map(Into::into),
            sct_params: params.sct_params.map(Into::into),
            shielded_pool_params: params.shielded_pool_params.map(Into::into),
            stake_params: params.stake_params.map(Into::into),
            dex_params: params.dex_params.map(Into::into),
        }
    }
}

/// Bundles together an "old" and "new" `ChangedAppParameters`
/// for storing in the JMT.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    try_from = "pb::ChangedAppParametersSet",
    into = "pb::ChangedAppParametersSet"
)]
pub struct ChangedAppParametersSet {
    pub old: ChangedAppParameters,
    pub new: ChangedAppParameters,
}

impl DomainType for ChangedAppParametersSet {
    type Proto = pb::ChangedAppParametersSet;
}

impl TryFrom<pb::ChangedAppParametersSet> for ChangedAppParametersSet {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ChangedAppParametersSet) -> anyhow::Result<Self> {
        Ok(ChangedAppParametersSet {
            old: msg
                .old
                .ok_or_else(|| anyhow::anyhow!("missing old parameters"))?
                .try_into()?,
            new: msg
                .new
                .ok_or_else(|| anyhow::anyhow!("missing new parameters"))?
                .try_into()?,
        })
    }
}

impl From<ChangedAppParametersSet> for pb::ChangedAppParametersSet {
    fn from(params: ChangedAppParametersSet) -> Self {
        pb::ChangedAppParametersSet {
            old: Some(params.old.into()),
            new: Some(params.new.into()),
        }
    }
}
