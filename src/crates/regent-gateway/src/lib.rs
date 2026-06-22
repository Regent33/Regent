//! regent-gateway — the messaging surface (canonical `features/gateway`).
//!
//! Clean-architecture internal layout: `domain/` (message events, session
//! keys, the shared command registry, the auth policy, adapter/handler
//! contracts), `application/` (the runner: routing, guards, approval
//! routing), `infra/` (platform adapters — Telegram first).
//!
//! The runner is platform-agnostic and conversation-agnostic: platforms
//! implement [`PlatformAdapter`]; the agent side implements
//! [`ConversationHandler`]. Every surface invariant from the design study
//! is harness code here: auth evaluated before anything else, bypass
//! commands reach the runner while an agent is busy, approvals resolve over
//! chat with deny-on-timeout.

pub mod application;
pub mod domain;
pub mod infra;

pub use application::approval::{ApprovalRouter, ChatApprovalHandler};
pub use application::runner::GatewayRunner;
pub use domain::auth::{AuthPolicy, AuthSnapshot};
pub use domain::contracts::{
    ConversationHandler, PlatformAdapter, SendAuth, SendBody, SendRequest, SyncReply,
    WebhookAdapter, WebhookRequest,
};
pub use domain::entities::{
    COMMAND_REGISTRY, CommandDef, MessageEvent, OutboundMessage, build_session_key, render_help,
    resolve_command,
};
pub use domain::errors::GatewayError;
pub use infra::speech_http::ReqwestExecutor;
pub use infra::platforms::azure_devops::AzureDevOpsAdapter;
pub use infra::platforms::discord::DiscordGateway;
pub use infra::platforms::email::EmailAdapter;
pub use infra::platforms::feishu::FeishuAdapter;
pub use infra::platforms::google_chat::GoogleChatAdapter;
pub use infra::platforms::jira::JiraAdapter;
pub use infra::platforms::line::LineAdapter;
pub use infra::platforms::mattermost::MattermostAdapter;
pub use infra::platforms::messenger::MessengerAdapter;
pub use infra::platforms::slack::SlackAdapter;
pub use infra::platforms::teams::TeamsAdapter;
pub use infra::platforms::telegram::TelegramAdapter;
pub use infra::platforms::trello::TrelloAdapter;
pub use infra::platforms::twilio_sms::TwilioSmsAdapter;
pub use infra::platforms::twilio_voice::TwilioVoiceAdapter;
pub use infra::platforms::wechat::WeChatAdapter;
pub use infra::platforms::wecom::WeComAdapter;
pub use infra::platforms::whatsapp::WhatsAppAdapter;
