use anyhow::Result;

use crate::{keys::OutgoingViewingKey, note};

use super::{SwapKey, SwapPlaintext, SWAP_CIPHERTEXT_BYTES, SWAP_LEN_BYTES};

#[derive(Debug, Clone)]
pub struct SwapCiphertext(pub [u8; SWAP_CIPHERTEXT_BYTES]);

impl SwapCiphertext {
    pub fn decrypt(
        &self,
        ovk: &OutgoingViewingKey,
        commitment: note::Commitment,
    ) -> Result<SwapPlaintext> {
        let swap_key = SwapKey::derive(ovk, commitment);
        self.decrypt_with_swap_key(&swap_key, commitment)
    }

    pub fn decrypt_with_swap_key(
        &self,
        swap_key: &SwapKey,
        commitment: note::Commitment,
    ) -> Result<SwapPlaintext> {
        let swap_ciphertext = self.0;
        let decryption_result = swap_key
            .decrypt(swap_ciphertext.to_vec(), commitment)
            .map_err(|_| anyhow::anyhow!("unable to decrypt swap ciphertext"))?;

        // TODO: encapsulate plaintext encoding by making this a
        // pub(super) parse_decryption method on SwapPlaintext
        // and removing the TryFrom impls
        let plaintext: [u8; SWAP_LEN_BYTES] = decryption_result
            .try_into()
            .map_err(|_| anyhow::anyhow!("swap decryption result did not fit in plaintext len"))?;

        plaintext.try_into().map_err(|_| {
            anyhow::anyhow!("unable to convert swap plaintext bytes into SwapPlaintext")
        })
    }
}

impl TryFrom<[u8; SWAP_CIPHERTEXT_BYTES]> for SwapCiphertext {
    type Error = anyhow::Error;

    fn try_from(bytes: [u8; SWAP_CIPHERTEXT_BYTES]) -> Result<SwapCiphertext, Self::Error> {
        Ok(SwapCiphertext(bytes))
    }
}

impl TryFrom<&[u8]> for SwapCiphertext {
    type Error = anyhow::Error;

    fn try_from(slice: &[u8]) -> Result<SwapCiphertext, Self::Error> {
        Ok(SwapCiphertext(slice[..].try_into()?))
    }
}
