use crate::common::*;

use message::extended;
use message::Message;
use peer::message;

pub(crate) trait Behaviour {
  fn handle_message(&mut self, message: &Message) -> Result<()> {
    match message.flavour {
      message::Flavour::Extended => self.handle_extended(message),
      _ => Ok(()),
    }
  }

  fn handle_extended(&mut self, message: &Message) -> Result<()> {
    let (id, payload) = message.parse_extended_payload()?;
    match id {
      extended::Id::Handshake => self.extension_handshake(payload),
      extended::Id::UtMetadata => self.ut_metadata(payload),
      extended::Id::NotImplemented(_) => Ok(()),
    }
  }

  fn ut_metadata(&mut self, payload: &[u8]) -> Result<()> {
    let msg: extended::UtMetadata = Message::from_bencode(payload)?;
    match msg.msg_type.into() {
      extended::ut_metadata::MsgType::Data => self.ut_metadata_data(msg, payload),
      extended::ut_metadata::MsgType::Request => self.ut_metadata_request(msg),
      _ => Ok(()),
    }
  }

  fn extension_handshake(&mut self, payload: &[u8]) -> Result<()>;
  fn ut_metadata_data(&mut self, msg: extended::UtMetadata, payload: &[u8]) -> Result<()>;
  fn ut_metadata_request(&mut self, msg: extended::UtMetadata) -> Result<()>;
}
