use soulseed_agi_envctx::{
    Anchor as EnvAnchor, EnvCtxProvisioner, EnvironmentContext, EnvironmentDataProvider,
    NoopReporter,
};

use crate::types::Anchor;

/// 将 Context 层 Anchor 转换为 envctx Anchor。
fn convert_anchor(anchor: &Anchor) -> EnvAnchor {
    EnvAnchor {
        tenant_id: anchor.tenant_id,
        envelope_id: anchor.envelope_id,
        config_snapshot_hash: anchor.config_snapshot_hash.clone(),
        config_snapshot_version: anchor.config_snapshot_version,
        session_id: anchor.session_id,
        sequence_number: anchor.sequence_number,
        access_class: anchor.access_class,
        provenance: anchor.provenance.clone(),
        schema_v: anchor.schema_v,
        supersedes: anchor.supersedes,
        superseded_by: anchor.superseded_by,
    }
}

/// 统一封装 envctx 装配逻辑，方便未来接入 Soulbase。
pub struct EnvContextGateway<P, R> {
    provisioner: EnvCtxProvisioner<P, R>,
}

impl<P, R> EnvContextGateway<P, R>
where
    P: EnvironmentDataProvider,
    R: soulseed_agi_envctx::DegradationReporter,
{
    pub fn new(provider: P, reporter: R) -> Self {
        Self {
            provisioner: EnvCtxProvisioner::new(provider, reporter),
        }
    }

    pub fn assemble(&self, anchor: &Anchor) -> soulseed_agi_envctx::Result<EnvironmentContext> {
        self.provisioner.assemble(convert_anchor(anchor))
    }

    pub fn provisioner(&self) -> &EnvCtxProvisioner<P, R> {
        &self.provisioner
    }
}

impl<P, R> From<EnvCtxProvisioner<P, R>> for EnvContextGateway<P, R>
where
    P: EnvironmentDataProvider,
    R: soulseed_agi_envctx::DegradationReporter,
{
    fn from(provisioner: EnvCtxProvisioner<P, R>) -> Self {
        Self { provisioner }
    }
}

/// 默认的 Soulbase Gateway 封装，使用 envctx 提供的 `SoulbaseEnvProvider`。
pub type SoulbaseEnvGateway<C> =
    EnvContextGateway<soulseed_agi_envctx::SoulbaseEnvProvider<C>, NoopReporter>;

impl<C> SoulbaseEnvGateway<C>
where
    C: soulseed_agi_envctx::ThinWaistClient,
{
    pub fn with_client(client: C) -> Self {
        let provider = soulseed_agi_envctx::SoulbaseEnvProvider::new(client);
        let reporter = NoopReporter;
        EnvCtxProvisioner::new(provider, reporter).into()
    }
}
