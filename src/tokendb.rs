use anyhow::{anyhow, bail, Context, Error, Result};
use rand::Fill;
use sled::transaction::{ConflictableTransactionError, TransactionError};
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

pub struct TokenDB {
    db: sled::Db,
}

pub struct Token([u8; 8]);

struct TokenData {
    uid: String,
    last_use: SystemTime,
}

fn to_abort(
    e: anyhow::Error,
) -> ConflictableTransactionError<Box<dyn std::error::Error + Send + Sync + 'static>> {
    ConflictableTransactionError::Abort(e.into())
}

fn from_abort(
    e: TransactionError<Box<dyn std::error::Error + Send + Sync + 'static>>,
) -> anyhow::Error {
    match e {
        TransactionError::Abort(e) => Error::msg(e.to_string()),
        TransactionError::Storage(e) => e.into(),
    }
}

impl TokenDB {
    pub fn open(path: &Path) -> Result<TokenDB> {
        Ok(TokenDB {
            db: sled::open(path).context("Failed to open token database")?,
        })
    }

    // pub fn get_token_of_user(&self, uid: &str) -> Result<Option<Token>> {
    //     if let Some(old_token) = self.db.get(format!("token_by_uid/{}", uid).as_bytes())? {
    //         Ok(Some(Token::try_from_ref(old_token.as_ref())?))
    //     } else {
    //         Ok(None)
    //     }
    // }

    pub fn create_token_for_user(&self, uid: &str) -> Result<Token> {
        let token = Token::random()?;

        self.db
            .transaction(|tx_db: &sled::transaction::TransactionalTree| {
                if let Some(old_token) = tx_db.get(format!("token_by_uid/{}", uid).as_bytes())? {
                    let old_token = Token::try_from_ref(old_token.as_ref()).map_err(to_abort)?;
                    return Err(to_abort(anyhow!(
                        "You already have a token: {}",
                        old_token.to_string()
                    )));
                }

                tx_db.insert(format!("token_by_uid/{}", uid).as_bytes(), &token.0)?;

                tx_db.insert(
                    &token.0,
                    TokenData {
                        uid: uid.to_string(),
                        last_use: SystemTime::UNIX_EPOCH,
                    }
                    .try_to_buf()
                    .map_err(to_abort)?,
                )?;

                Ok(())
            })
            .map_err(from_abort)?;

        Ok(token)
    }

    pub fn try_use_token(&self, token: Token, min_interval: Duration) -> Result<()> {
        self.db
            .transaction(|tx_db: &sled::transaction::TransactionalTree| {
                let now = SystemTime::now();

                let data = TokenData::try_from_buf(
                    tx_db
                        .get(&token.0)?
                        .context("This token does not exist")
                        .map_err(to_abort)?
                        .as_ref(),
                )
                .map_err(to_abort)?;

                let duration = now.duration_since(data.last_use).unwrap_or(Duration::ZERO);
                if duration < min_interval {
                    return Err(to_abort(anyhow!(
                        "Cooldown period is {:?}, you have to wait {:?} more",
                        min_interval,
                        min_interval - duration
                    )));
                }

                tx_db.insert(
                    &token.0,
                    TokenData {
                        uid: data.uid,
                        last_use: now,
                    }
                    .try_to_buf()
                    .map_err(to_abort)?,
                )?;

                Ok(())
            })
            .map_err(from_abort)?;
        Ok(())
    }
}

impl Token {
    fn random() -> Result<Token> {
        let mut token = Token([0u8; 8]);
        token.0[0] = 0xff; // a character outside ASCII to avoid collisions
        token.0[1..7]
            .try_fill(&mut rand::thread_rng())
            .context("Failed to generate random token")?;
        Ok(token)
    }

    fn try_from_ref(data: &[u8]) -> Result<Token> {
        let mut token = Token([0u8; 8]);
        if data.len() != token.0.len() {
            bail!("Invalid token length");
        }
        token.0.copy_from_slice(data);
        Ok(token)
    }

    pub fn try_from_string(s: &str) -> Result<Token> {
        let mut token = Token([0u8; 8]);
        if s.len() != token.0.len() * 2 {
            bail!("Invalid token length");
        }
        for i in 0..token.0.len() {
            token.0[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).context("Invalid token")?;
        }
        Ok(token)
    }

    pub fn to_string(&self) -> String {
        let mut s = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            write!(s, "{:02x}", byte).unwrap();
        }
        s
    }
}

impl TokenData {
    fn try_from_buf(buf: &[u8]) -> Result<TokenData> {
        if buf.len() < 12 {
            bail!("Token data is too short");
        }

        let version = u32::from_le_bytes(buf[..4].try_into().unwrap());
        match version {
            1 => {
                let last_use_timestamp = u64::from_le_bytes(buf[4..12].try_into().unwrap());
                let last_use = SystemTime::UNIX_EPOCH + Duration::from_millis(last_use_timestamp);

                let uid =
                    String::from_utf8((&buf[12..]).to_vec()).context("Failed to parse UID")?;

                Ok(TokenData { uid, last_use })
            }
            _ => bail!("Unknown token version {}", version),
        }
    }

    fn try_to_buf(&self) -> Result<Vec<u8>> {
        let uid = self.uid.as_bytes();
        let mut data = Vec::with_capacity(12 + uid.len());
        data.write(&1u32.to_le_bytes())?;
        data.write(
            &(self
                .last_use
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_millis() as u64)
                .to_le_bytes(),
        )?;
        data.write(uid)?;
        Ok(data)
    }
}
