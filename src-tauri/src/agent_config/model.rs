//! Domain model for configuration sync: skills and MCP connectors discovered
//! across agent clients, the differences between them, and the plans/receipts
//! that make a copy safe (architecture.md section 13, design.md).
//!
//! Normalization identifies comparable items (task 1.3) but never discards a
//! client's own fields: [`RawItem::extra`] carries every field GitWyrm does
//! not understand so a write can round-trip them untouched.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Which agent client a location/state belongs to. Kept as a fixed enum
/// (rather than a free string) so the UI's per-client columns and the
/// writers dispatch table are exhaustive-checked by the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum ClientId {
    Codex,
    ClaudeCode,
    OpenCode,
    VsCodeCopilot,
    OpenChamber,
}

impl ClientId {
    pub const ALL: [ClientId; 5] = [
        ClientId::Codex,
        ClientId::ClaudeCode,
        ClientId::OpenCode,
        ClientId::VsCodeCopilot,
        ClientId::OpenChamber,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ClientId::Codex => "Codex",
            ClientId::ClaudeCode => "Claude",
            ClientId::OpenCode => "OpenCode",
            ClientId::VsCodeCopilot => "Copilot",
            ClientId::OpenChamber => "OpenChamber",
        }
    }
}

/// The kind of configuration item being synced. `Skill` covers Agent Skills
/// / prompt-style capabilities; `McpConnector` covers MCP server entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    Skill,
    McpConnector,
}

/// Where a discovered item's configuration file lives, on this machine, for
/// one client. `scope` distinguishes a personal (user-home) location from a
/// this-repository location so the same skill name can exist at both scopes
/// without colliding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigLocation {
    pub client: ClientId,
    pub scope: ConfigScope,
    /// Absolute path to the file this item's configuration lives in (or would
    /// be written to). Never a directory: writers always target one file.
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ConfigScope {
    Personal,
    Repo,
}

/// A field GitWyrm does not model explicitly, preserved so a write never
/// silently drops client-specific configuration (task 1.3). Values are kept
/// as opaque JSON so nested client-specific shapes survive untouched.
pub type ExtraFields = std::collections::BTreeMap<String, JsonValue>;

/// `serde_json::Value`, wrapped so it can cross the specta/TS binding
/// boundary. `serde_json::Value` has no `specta::Type` impl in this
/// workspace (the `specta`/`serde_json` integration feature is not enabled,
/// and enabling it workspace-wide was judged too broad a change for this
/// feature alone) -- [`JsonValue`] transparently serializes exactly like the
/// inner `Value` (so every reader/writer/redaction function in this module
/// works with it as plain JSON) while exporting to TypeScript as `unknown`,
/// which is what the frontend should treat arbitrary client-specific
/// configuration as anyway.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonValue(pub serde_json::Value);

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        JsonValue(value)
    }
}
impl From<JsonValue> for serde_json::Value {
    fn from(value: JsonValue) -> Self {
        value.0
    }
}
impl std::ops::Deref for JsonValue {
    type Target = serde_json::Value;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for JsonValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Serialize for JsonValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(JsonValue(serde_json::Value::deserialize(deserializer)?))
    }
}
impl Type for JsonValue {
    fn inline(_: &mut specta::TypeCollection, _: specta::Generics) -> specta::datatype::DataType {
        specta::datatype::DataType::Unknown
    }
}

/// One field that may carry a secret (a token, API key, header value, or
/// command argument that looks like a credential). The identity of the field
/// is kept; the value never is (task 1.5, spec "Secrets are not spread
/// silently").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SecretFieldRef {
    /// Dotted path within the item's normalized fields, e.g. `env.API_KEY` or
    /// `headers.Authorization`.
    pub field_path: String,
    /// Why this field is treated as secret: env var, header, command arg, or
    /// a field literally named token/key/secret/password.
    pub reason: SecretReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SecretReason {
    EnvironmentValue,
    HeaderValue,
    CommandArgument,
    NamedSecretField,
}

/// One discovered item, normalized enough to compare across clients while
/// retaining the client's own raw fields and source path (task 1.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RawItem {
    pub location: ConfigLocation,
    pub kind: ItemKind,
    /// Stable identity used to match the "same" item across clients: for a
    /// skill this is its directory/file name; for an MCP connector, its
    /// configured server name. Comparison is case-sensitive and exact --
    /// fuzzy matching would risk merging two genuinely different items.
    pub identity: String,
    pub display_name: String,
    pub description: Option<String>,
    /// Every field this client's schema defines, normalized keys where
    /// GitWyrm understands them (`command`, `args`, `env`, `url`, `headers`)
    /// plus anything else under its original key in `extra`.
    pub extra: ExtraFields,
    pub secret_fields: Vec<SecretFieldRef>,
    /// Content hash of this item's raw serialized form, used to detect when
    /// an item has changed since it was last scanned (task 3.2 concurrent
    /// edit detection operates on the *file*, this is for the item itself).
    pub content_hash: String,
}

/// The full scan result: every item found, grouped for the UI by identity
/// across clients happens in the frontend/normalize layer, not here -- this
/// is the flat read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ConfigScanResult {
    pub items: Vec<RawItem>,
    pub detections: Vec<ClientDetection>,
}

/// Whether a client was found on this machine at all, independent of whether
/// it has any items configured yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientDetection {
    pub client: ClientId,
    pub present: bool,
    /// `false` once a merge writer exists and is wired in [`super::writers`].
    /// Drives the "Unsupported" badge and keeps discovery-only clients from
    /// ever reaching an apply path (task 4.6).
    pub write_supported: bool,
}

/// One item's comparison state against a chosen source, for one destination
/// client (task 1.4). The mockup's badge vocabulary is richer than a plain
/// same/different/missing split, so every observed badge text maps to a
/// distinct variant rather than being collapsed:
///
/// - `Synced` -> [`ClientSyncState::Same`]
/// - `Different` -> [`ClientSyncState::Different`]
/// - `Missing` -> [`ClientSyncState::Missing`]
/// - `Older` -> [`ClientSyncState::Outdated`] (present, differs, and is behind the
///   source by a detectable version/update marker)
/// - `Source` -> [`ClientSyncState::IsSource`] (this destination *is* the chosen
///   source item; never a copy target for itself)
/// - `Unsupported` -> [`ClientSyncState::Unsupported`] (client detected, but no
///   writer exists, or the item's shape cannot be represented there)
/// - `Own copy` -> [`ClientSyncState::KeptSeparate`] (deliberately excluded from
///   sync, e.g. user marked it client-specific; still "good" in the UI)
/// - `Needs setup` -> [`ClientSyncState::NeedsSetup`] (client present, matching
///   item present, but missing a required piece such as an MCP command that
///   is not yet runnable there)
/// - `Off` -> [`ClientSyncState::Disabled`] (client detected but the user turned
///   sync off for it, or the client itself has this feature disabled)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ClientSyncState {
    Same,
    Different,
    Missing,
    Outdated,
    IsSource,
    Unsupported,
    KeptSeparate,
    NeedsSetup,
    Disabled,
    /// Client not detected on this machine at all -- distinct from `Missing`
    /// (client present, item absent) so the UI can explain the difference.
    ClientNotDetected,
}

impl ClientSyncState {
    /// `good` / `warn` / `missing` maps directly to the mockup's three CSS
    /// classes (`ag-sync-state good|warn|missing`), so the frontend can
    /// derive styling from state alone.
    pub fn badge_class(self) -> &'static str {
        match self {
            ClientSyncState::Same | ClientSyncState::IsSource | ClientSyncState::KeptSeparate => "good",
            ClientSyncState::Different | ClientSyncState::Outdated | ClientSyncState::NeedsSetup => "warn",
            ClientSyncState::Missing
            | ClientSyncState::Unsupported
            | ClientSyncState::Disabled
            | ClientSyncState::ClientNotDetected => "missing",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ClientSyncState::Same => "Synced",
            ClientSyncState::Different => "Different",
            ClientSyncState::Missing => "Missing",
            ClientSyncState::Outdated => "Older",
            ClientSyncState::IsSource => "Source",
            ClientSyncState::Unsupported => "Unsupported",
            ClientSyncState::KeptSeparate => "Own copy",
            ClientSyncState::NeedsSetup => "Needs setup",
            ClientSyncState::Disabled => "Off",
            ClientSyncState::ClientNotDetected => "Not detected",
        }
    }
}

/// One row of the inventory table: one logical item (matched by identity
/// across clients) plus its chosen source and per-client state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryEntry {
    pub item_id: String,
    pub kind: ItemKind,
    pub display_name: String,
    pub scope: ConfigScope,
    /// The client currently treated as this item's source of truth --
    /// usually where it was first found, but can be "this repository" for a
    /// repo-scoped `.agents`-style location.
    pub source: ItemSource,
    pub per_client: Vec<ClientSyncStatus>,
    pub has_secrets: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ItemSource {
    Client { client: ClientId },
    /// A repository-scoped source not owned by any single client app, e.g.
    /// an `.agents` directory checked into the repo (mockup: "source: this
    /// repository" / "source: .agents").
    Repository,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ClientSyncStatus {
    pub client: ClientId,
    pub state: ClientSyncState,
}

/// Aggregate counts shown in the setup view's summary line (mockup: "18
/// skills found - 11 match - 3 differ - 4 exist in one app").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InventorySummary {
    pub total: u32,
    pub matching: u32,
    pub differing: u32,
    pub exists_in_one: u32,
}

// -- Preview / apply / undo (tasks 3.1-3.5) --

/// One destination's computed preview: the exact content GitWyrm proposes to
/// write, the current file hash it was computed against, and any warnings.
/// Nothing here is applied until [`ApplyRequest`] confirms this exact hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DestinationPreview {
    pub client: ClientId,
    pub destination_path: String,
    /// `None` when the destination file does not exist yet (a create, not a
    /// merge).
    pub before_hash: Option<String>,
    pub proposed_content: String,
    /// Redacted rendering safe to show in the UI: secret values already
    /// replaced with a marker before this ever reaches the frontend log or
    /// the plan file (task 1.5, 5.4).
    pub redacted_diff_summary: Vec<ChangeSummaryLine>,
    pub warnings: Vec<PlanWarning>,
    pub write_supported: bool,
}

/// One human-readable line describing a semantic change, with secret values
/// already redacted. Never the raw file diff -- a line names the field and
/// what kind of change happened, not arbitrary content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeSummaryLine {
    pub field_path: String,
    pub change: FieldChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FieldChangeKind {
    Added,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// The item carries secret fields that will not be copied as literal
    /// values into this destination's format.
    SecretNotCopied,
    /// The destination format cannot represent something the source has;
    /// the write will proceed but drop that piece (named in the warning
    /// alongside this variant by the caller-facing message, kept out of this
    /// enum since the message text is UI copy, not the typed reason).
    UnsupportedField,
    /// The destination client was not detected as installed; the file would
    /// still be written to the conventional path.
    ClientNotDetected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanWarning {
    pub kind: WarningKind,
    /// Plain-language explanation, already redacted -- never contains a
    /// secret value.
    pub message: String,
}

/// A reviewed, not-yet-applied plan: one item copied to one or more
/// destinations. Persisted so `agent_config_apply_copy(plan_id)` can look it
/// up, and so "Match selected apps" is built from the same plans as a
/// single-item copy (task 2.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CopyPlan {
    pub plan_id: String,
    pub item_id: String,
    pub source_item: RawItem,
    pub destinations: Vec<DestinationPreview>,
    pub created_at: String,
}

/// The redacted view of a [`CopyPlan`] sent to the frontend. `CopyPlan`
/// itself carries `source_item.extra` (every field GitWyrm does not
/// understand, including secret values before redaction -- see
/// [`RawItem::extra`]) and `DestinationPreview::proposed_content` (the exact
/// bytes about to be written, which for a supported writer embed those same
/// unredacted values). Neither may cross the IPC boundary or be shown to the
/// UI: only [`DestinationPreview::redacted_diff_summary`] is redaction-safe.
/// This type carries everything the UI needs to render [`super::super`]'s
/// `PlanReview`/`CopyPreviewDialog` (destination, path, warnings, redacted
/// diff, write support) and nothing else. The full [`CopyPlan`] stays on the
/// backend, persisted by [`super::plan::SafeWriteRoot::plans_dir`] exactly as
/// before, so `agent_config_apply_copy`/`agent_config_apply_batch` still read
/// the real content to write -- only what leaves the process for display is
/// narrowed here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RedactedCopyPlan {
    pub plan_id: String,
    pub item_id: String,
    pub source_client: ClientId,
    pub source_has_secrets: bool,
    pub destinations: Vec<RedactedDestinationPreview>,
    pub created_at: String,
}

/// [`DestinationPreview`] with `proposed_content` (raw file bytes, potentially
/// carrying real secret values) dropped. Every other field is already
/// redaction-safe by construction: `redacted_diff_summary` is built from
/// [`super::redact::redact_for_display`] output, and `warnings` messages are
/// plain UI copy that never embeds a field value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RedactedDestinationPreview {
    pub client: ClientId,
    pub destination_path: String,
    pub has_before_content: bool,
    pub redacted_diff_summary: Vec<ChangeSummaryLine>,
    pub warnings: Vec<PlanWarning>,
    pub write_supported: bool,
}

impl From<&CopyPlan> for RedactedCopyPlan {
    fn from(plan: &CopyPlan) -> Self {
        RedactedCopyPlan {
            plan_id: plan.plan_id.clone(),
            item_id: plan.item_id.clone(),
            source_client: plan.source_item.location.client,
            source_has_secrets: !plan.source_item.secret_fields.is_empty(),
            destinations: plan.destinations.iter().map(RedactedDestinationPreview::from).collect(),
            created_at: plan.created_at.clone(),
        }
    }
}

impl From<&DestinationPreview> for RedactedDestinationPreview {
    fn from(dest: &DestinationPreview) -> Self {
        RedactedDestinationPreview {
            client: dest.client,
            destination_path: dest.destination_path.clone(),
            has_before_content: dest.before_hash.is_some(),
            redacted_diff_summary: dest.redacted_diff_summary.clone(),
            warnings: dest.warnings.clone(),
            write_supported: dest.write_supported,
        }
    }
}

/// Outcome of computing a preview. Carries [`RedactedCopyPlan`], never the
/// full [`CopyPlan`] -- see that type's doc comment for why (task 1.5/5.4:
/// secret values must never cross the IPC boundary or be persisted to a
/// frontend-visible log).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PreviewOutcome {
    Ready { plan: RedactedCopyPlan },
    ItemNotFound,
    NoDestinations,
}

/// Per-destination result of an apply pass. One [`ApplyOutcome`] can contain
/// a mix of `Applied` and `Refused`/`Failed` entries -- a batch never treats
/// partial success as an all-or-nothing unit (task 5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DestinationApplyResult {
    #[serde(rename_all = "camelCase")]
    Applied {
        client: ClientId,
        operation_id: String,
        receipt: OperationReceipt,
    },
    /// The destination file changed since preview was computed. Nothing was
    /// written; the caller should refresh the plan and try again.
    #[serde(rename_all = "camelCase")]
    ConcurrentChangeRefused {
        client: ClientId,
        expected_hash: Option<String>,
        actual_hash: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    WriteFailed {
        client: ClientId,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcome {
    pub plan_id: String,
    pub results: Vec<DestinationApplyResult>,
}

/// Everything needed to undo one write, byte-for-byte (task 3.4, 5.1).
/// Backups are stored under the app data directory, never beside the
/// destination file, so a corrupted client directory cannot also destroy the
/// recovery copy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationReceipt {
    pub operation_id: String,
    pub plan_id: String,
    pub client: ClientId,
    pub destination_path: String,
    /// Hash of the destination's content immediately before this write (the
    /// same value the write was gated on). `None` if the file did not exist
    /// before this write (undo then means "delete the file we created").
    pub before_hash: Option<String>,
    pub after_hash: String,
    /// Path to the byte-identical backup of the pre-write content, under app
    /// data. Absent when `before_hash` is `None` (nothing to back up).
    pub backup_path: Option<String>,
    pub applied_at: String,
    pub undone: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UndoOutcome {
    Restored { receipt: OperationReceipt },
    AlreadyUndone,
    OperationNotFound,
    /// The destination changed since the write this receipt describes, so
    /// undoing would clobber someone else's newer edit. Nothing was touched.
    #[serde(rename_all = "camelCase")]
    ConcurrentChangeRefused {
        expected_hash: String,
        actual_hash: Option<String>,
    },
    RestoreFailed {
        detail: String,
    },
}

/// Request shape for `agent_config_apply_copy`'s batch form ("Match selected
/// apps"): a list of plan IDs, each already previewed individually. There is
/// no separate "sync everything" code path -- a batch is just several plans
/// applied in sequence (task 2.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchApplyRequest {
    pub plan_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BatchApplyOutcome {
    pub outcomes: Vec<ApplyOutcome>,
}
