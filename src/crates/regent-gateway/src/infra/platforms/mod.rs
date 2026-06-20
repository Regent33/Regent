//! Per-platform messaging adapters. Polling (Telegram) and webhook (Messenger,
//! LINE, WhatsApp, Slack, Mattermost) platforms each normalize their wire
//! format to the shared `MessageEvent`/`OutboundMessage` types. Parse/verify/
//! build are pure (unit-testable without tokens); only the live send needs
//! credentials.

pub mod azure_devops;
pub mod discord;
pub mod email;
pub mod feishu;
pub mod feishu_crypto;
pub mod google_chat;
pub mod jira;
pub mod line;
pub mod mattermost;
pub mod messenger;
pub mod slack;
pub mod teams;
pub mod telegram;
pub mod trello;
pub mod twilio;
pub mod twilio_sms;
pub mod twilio_voice;
pub mod wechat;
pub mod wechat_crypto;
pub mod wecom;
pub mod whatsapp;
