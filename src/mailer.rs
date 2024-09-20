use crate::{config::SmtpSettings, models::PartData, reports::generate_html_report};
use async_smtp::{
    authentication::{Credentials, Mechanism, DEFAULT_ENCRYPTED_MECHANISMS},
    Envelope, SendableEmail, SmtpClient, SmtpTransport,
};
use eyre::Result;
use tokio::{io::BufStream, net::TcpStream};
use tracing::{debug, error};

pub struct Mailer {
    transport: SmtpTransport<BufStream<TcpStream>>,
    creds: Credentials,
    envelope: Envelope,
}

impl Mailer {
    pub async fn new(settings: &SmtpSettings) -> Result<Self> {
        let stream = BufStream::new(
            TcpStream::connect(format!("{}:{}", settings.server, settings.port)).await?,
        );
        let client = SmtpClient::new();

        let transport = SmtpTransport::new(client, stream).await?;
        let creds = Credentials::new(settings.username.clone(), settings.password.clone());
        let envelope = Envelope::new(
            Some(settings.from.parse().unwrap()),
            settings.to.iter().map(|r| r.parse()).flatten().collect(),
        )
        .unwrap();

        Ok(Self {
            transport,
            creds,
            envelope,
        })
    }

    pub async fn send_report(
        &mut self,
        subject: &str,
        parts: &[PartData],
        sender_name: &str,
    ) -> Result<()> {
        
        let html_body = generate_html_report(parts)?;
        let email = SendableEmail::new(
            self.envelope.clone(),
            self.format_email(subject, html_body, sender_name)?
                .as_bytes()
                .to_vec(),
        );
        // Попытка логина с механизмами по умолчанию
        match self
            .transport
            .try_login(&self.creds, DEFAULT_ENCRYPTED_MECHANISMS)
            .await
        {
            Ok(_) => {
                debug!("Login successful using DEFAULT_ENCRYPTED_MECHANISMS");
            }
            Err(try_login_err) => {
                error!("Try Login Error: {try_login_err:#?}");
                // Logout и повторная попытка с Plain
                if let Err(logout_err) = self.transport.quit().await {
                    error!("Logout Error: {logout_err:#?}");
                    return Err(logout_err.into());
                }

                self.transport.auth(Mechanism::Plain, &self.creds).await?;
                debug!("Authenticated using Mechanism::Plain");
            }
        }

        // Отправка письма
        if let Err(send_err) = self.transport.send(email).await {
            error!("Email send error: {send_err:#?}");
        } else {
            debug!("Email sent successfully");
        }

        Ok(())
    }

    /// Формирует строку с заголовками и телом письма
    pub fn format_email(&self, subject: &str, body: String, sender_name: &str) -> Result<String> {
        let from_email = self
            .envelope
            .from()
            .ok_or_else(|| eyre::eyre!("Invalid from email"))?
            .to_string();
        let from = format!("\"{}\" <{}>", sender_name, from_email);
        let to = self
            .envelope
            .to()
            .iter()
            .map(|addr| addr.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        // Формируем строку с заголовками и телом письма
        Ok(format!(
            "From: {}\r\nTo: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{}",
            from, to, subject, body
        ))
    }
}
