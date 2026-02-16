use cosmwasm_std::{CosmosMsg, StdResult, Binary, StdError};
use prost::Message;

/// ZigChain TokenFactory message structures using prost
/// These match the exact proto definitions from ZigChain

#[derive(Clone, PartialEq, Message)]
pub struct MsgCreateDenom {
    #[prost(string, tag = "1")]
    pub sender: String,
    #[prost(string, tag = "2")]
    pub subdenom: String,
    #[prost(string, optional, tag = "3")]
    pub minting_cap: Option<String>,
    #[prost(bool, tag = "4")]
    pub can_change_minting_cap: bool,
    #[prost(string, optional, tag = "5")]
    pub uri: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub uri_hash: Option<String>,
    #[prost(string, optional, tag = "7")]
    pub description: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Coin {
    #[prost(string, tag = "1")]
    pub denom: String,
    #[prost(string, tag = "2")]
    pub amount: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgMintAndSendTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
    #[prost(string, tag = "3")]
    pub recipient: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct MsgBurnTokens {
    #[prost(string, tag = "1")]
    pub signer: String,
    #[prost(message, optional, tag = "2")]
    pub token: Option<Coin>,
}

/// Create a new denom using ZigChain TokenFactory
/// Returns a Stargate CosmosMsg that can be added to Response
pub fn create_denom_msg(
    creator: String,
    subdenom: String,
    minting_cap: String,
    can_change_minting_cap: bool,
    uri: Option<String>,
    uri_hash: Option<String>,
    description: Option<String>,
) -> StdResult<CosmosMsg> {
    let msg = MsgCreateDenom {
        sender: creator,
        subdenom,
        minting_cap: Some(minting_cap),
        can_change_minting_cap,
        uri,
        uri_hash,
        description,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgCreateDenom: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgCreateDenom".to_string(),
        value: Binary::from(buf),
    })
}

/// Mint tokens and send to recipient using TokenFactory
pub fn mint_and_send_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
    recipient: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgMintAndSendTokens {
        signer,
        token: Some(Coin { denom, amount }),
        recipient,
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgMintAndSendTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgMintAndSendTokens".to_string(),
        value: Binary::from(buf),
    })
}

/// Burn tokens using TokenFactory
pub fn burn_tokens_msg(
    signer: String,
    denom: String,
    amount: String,
) -> StdResult<CosmosMsg> {
    let msg = MsgBurnTokens {
        signer,
        token: Some(Coin { denom, amount }),
    };

    let mut buf = Vec::new();
    msg.encode(&mut buf)
        .map_err(|e| StdError::generic_err(format!("Failed to encode MsgBurnTokens: {}", e)))?;

    #[allow(deprecated)]
    Ok(CosmosMsg::Stargate {
        type_url: "/zigchain.factory.MsgBurnTokens".to_string(),
        value: Binary::from(buf),
    })
}
