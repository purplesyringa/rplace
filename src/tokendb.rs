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

pub struct Token(String);

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

    pub fn add_token(&self, token: Token, uid: &str) -> Result<Token> {
        self.db
            .transaction(|tx_db: &sled::transaction::TransactionalTree| {
                if let Some(old_token) = tx_db.get(format!("token_by_uid/{}", uid).as_bytes())? {
                    let old_token = Token::try_from_bytes(old_token.as_ref()).map_err(to_abort)?;
                    return Err(to_abort(anyhow!(
                        "You already have a token: {}",
                        old_token.to_string()
                    )));
                }

                tx_db.insert(format!("token_by_uid/{}", uid).as_bytes(), token.to_bytes())?;

                if let Some(old_token_data) = tx_db.insert(
                    token.to_bytes(),
                    TokenData {
                        uid: uid.to_string(),
                        last_use: SystemTime::UNIX_EPOCH,
                    }
                    .try_to_buf()
                    .map_err(to_abort)?,
                )? {
                    let old_token =
                        TokenData::try_from_buf(old_token_data.as_ref()).map_err(to_abort)?;
                    return Err(to_abort(anyhow!(
                        "This token is already registered to user {:?}",
                        old_token.uid
                    )));
                }

                Ok(())
            })
            .map_err(from_abort)?;

        Ok(token)
    }

    pub fn create_token_for_user(&self, uid: &str) -> Result<Token> {
        self.add_token(Token::random()?, uid)
    }

    pub fn try_use_token(&self, token: Token, min_interval: Duration) -> Result<()> {
        self.db
            .transaction(|tx_db: &sled::transaction::TransactionalTree| {
                let now = SystemTime::now();

                let data = TokenData::try_from_buf(
                    tx_db
                        .get(&token.to_bytes())?
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
                    token.to_bytes(),
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
        let mut buf = [0u8; 8];
        buf.try_fill(&mut rand::thread_rng())
            .context("Failed to generate random token")?;

        let mut s = String::with_capacity(buf.len() * 2);
        for byte in buf {
            write!(s, "{:02x}", byte).unwrap();
        }
        Ok(Token(s))
    }

    fn try_from_bytes(data: &[u8]) -> Result<Token> {
        if data[0] != 0xff {
            bail!("Invalid token format");
        }
        Ok(Token(
            String::from_utf8(data[1..].to_vec()).context("Failed to parse token")?,
        ))
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut vec = Vec::with_capacity(1 + self.0.as_bytes().len());
        vec.push(0xff);
        vec.extend_from_slice(self.0.as_bytes());
        vec
    }

    pub fn from_string(s: &str) -> Token {
        Token(s.to_string())
    }

    pub fn to_string(&self) -> String {
        self.0.clone()
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
