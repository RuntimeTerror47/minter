use crate::ContractError::Unauthorized;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::WasmMsg::Execute;
use cosmwasm_std::{
    to_binary, BankMsg, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, Uint64,
};
use cw2::set_contract_version;
use std::ops::Add;

pub type Cw721ExecuteMsg = cw721_base::ExecuteMsg<Metadata>;

use crate::error::ContractError;
use crate::helper::{generate_dragon_mint_msg, generate_egg_mint_msg};
use crate::msg::{
    ExecuteMsg, GetEggsaleInfoResponse, GetStateResponse, InstantiateMsg, Metadata, MintEggDragon,
    QueryMsg,
};
use crate::state::{ContractAddressList, State, CONTRACTS, EGG_SALE_COUNT, STATE, TOTAL_EGGS};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:minter";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        owner: String::from(info.sender.clone()),
        base_price: msg.base_price,
        hatch_price: msg.hatch_price,
        random_key: msg.random_key,
        egg_sale_size: msg.egg_sale_size,
    };

    let contracts = ContractAddressList {
        egg: "".to_string(),
        dragon: "".to_string(),
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;
    EGG_SALE_COUNT.save(deps.storage, &Uint64::new(0))?;
    TOTAL_EGGS.save(deps.storage, &Uint64::new(0))?;
    CONTRACTS.save(deps.storage, &contracts)?;

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::MintEgg(msg) => execute_egg_mint(deps, info, msg),
        ExecuteMsg::HatchEgg(msg) => execute_hatch_egg(deps, info, msg),
        ExecuteMsg::GenesisHatch(msg) => execute_free_hatch(deps, info, msg),
        ExecuteMsg::DragonBirth { id, owner } => execute_dragon_birth(deps, info, id, owner),
        ExecuteMsg::EditState {
            base_price,
            random_key,
            hatch_price,
            egg_sale_size,
        } => execute_edit_state(
            deps,
            info,
            base_price,
            random_key,
            hatch_price,
            egg_sale_size,
        ),
        ExecuteMsg::EditContracts { egg, dragon } => {
            execute_edit_contracts(deps, info, egg, dragon)
        }
    }
}

pub fn execute_free_hatch(
    deps: DepsMut,
    info: MessageInfo,
    msg: MintEggDragon,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;

    if state.hatch_price != Uint128::new(0) && info.sender != state.owner {
        return Err(ContractError::Unauthorized {
            msg: "free hatch is no longer available".to_string(),
        });
    }
    let contracts = CONTRACTS.load(deps.storage)?;
    let dragon = contracts.dragon;
    let egg = contracts.egg;

    //id
    let id_type = msg.clone().id;
    let egg_id = &id_type[id_type.len() - 5..id_type.len()];

    //dragon type
    let key1 = state.random_key / 10000;
    let key2 = state.random_key % 100;
    let secret = key1 + key2;
    let type_id = id_type.chars().nth(secret as usize).unwrap();

    let type_name;
    match type_id {
        '2' => type_name = "uncommon",
        '3' => type_name = "rare",
        '4' => type_name = "epic",
        '5' => type_name = "legendary",
        'P' => type_name = "legendary",
        'K' => type_name = "epic",
        'G' => type_name = "rare",
        'W' => type_name = "uncommon",
        _ => type_name = "common",
    };

    let dragon_mint = generate_dragon_mint_msg(
        egg_id.clone(),
        type_name.to_string(),
        info.sender.to_string(),
    )?;

    let transfer_egg = Cw721ExecuteMsg::TransferNft {
        recipient: state.owner,
        token_id: String::from(egg_id.clone()),
    };

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(Execute {
            contract_addr: egg,
            msg: to_binary(&transfer_egg)?,
            funds: vec![],
        }))
        .add_message(CosmosMsg::Wasm(Execute {
            contract_addr: dragon,
            msg: to_binary(&dragon_mint)?,
            funds: vec![],
        }))
        .add_attribute("egg burn", msg.id)
        .add_attribute("mint dragon for", info.sender))
}

pub fn execute_hatch_egg(
    deps: DepsMut,
    info: MessageInfo,
    msg: MintEggDragon,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let contracts = CONTRACTS.load(deps.storage)?;
    let dragon = contracts.dragon;
    let egg = contracts.egg;

    let payment;

    if info.funds.len() != 1 {
        return Err(ContractError::SendSingleNativeToken {});
    }

    let sent_fund = info.funds.get(0).unwrap();
    if sent_fund.denom != "ujunox" {
        return Err(ContractError::NativeDenomNotAllowed {
            denom: sent_fund.clone().denom,
        });
    }

    let fee = Uint128::from(state.hatch_price);

    // check price matches
    if fee != sent_fund.amount {
        return Err(ContractError::SentWrongFundsAmount {
            need: fee,
            sent: sent_fund.amount,
        });
    }

    payment = BankMsg::Send {
        to_address: state.owner.clone(),
        amount: vec![Coin {
            denom: "ujunox".to_string(),
            amount: fee,
        }],
    };

    //id
    let id_type = msg.clone().id;
    let egg_id = &id_type[id_type.len() - 5..id_type.len()];

    //dragon type
    let key1 = state.random_key / 10000;
    let key2 = state.random_key % 100;
    let secret = key1 + key2;
    let type_id = id_type.chars().nth(secret as usize).unwrap();

    let type_name;
    match type_id {
        '2' => type_name = "uncommon",
        '3' => type_name = "rare",
        '4' => type_name = "epic",
        '5' => type_name = "legendary",
        'P' => type_name = "legendary",
        'K' => type_name = "epic",
        'G' => type_name = "rare",
        'W' => type_name = "uncommon",
        _ => type_name = "common",
    };

    let dragon_mint = generate_dragon_mint_msg(
        egg_id.clone(),
        type_name.to_string(),
        info.sender.to_string(),
    )?;

    let transfer_egg = Cw721ExecuteMsg::TransferNft {
        recipient: state.owner,
        token_id: String::from(egg_id.clone()),
    };

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(Execute {
            contract_addr: egg,
            msg: to_binary(&transfer_egg)?,
            funds: vec![],
        }))
        .add_message(CosmosMsg::Wasm(Execute {
            contract_addr: dragon,
            msg: to_binary(&dragon_mint)?,
            funds: vec![],
        }))
        .add_messages(vec![payment])
        .add_attribute("egg burn", msg.id)
        .add_attribute("mint dragon for", info.sender))
}

pub fn execute_egg_mint(
    deps: DepsMut,
    info: MessageInfo,
    msg: MintEggDragon,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    let contracts = CONTRACTS.load(deps.storage)?;
    let count = EGG_SALE_COUNT.load(deps.storage)?;

    if count >= state.egg_sale_size {
        return Err(ContractError::EggSaleLimit {});
    }

    if info.funds.len() != 1 {
        return Err(ContractError::SendSingleNativeToken {});
    }

    let sent_fund = info.funds.get(0).unwrap();
    if sent_fund.denom != "ujunox" {
        return Err(ContractError::NativeDenomNotAllowed {
            denom: sent_fund.clone().denom,
        });
    }

    let fee = Uint128::from(state.base_price);

    // check price matches
    if fee != sent_fund.amount {
        return Err(ContractError::SentWrongFundsAmount {
            need: fee,
            sent: sent_fund.amount,
        });
    }

    let payment = BankMsg::Send {
        to_address: state.owner.clone(),
        amount: vec![Coin {
            denom: "ujunox".to_string(),
            amount: fee,
        }],
    };

    //id_type contains 100digit type and id
    let id_type = msg.clone().id;
    let id = &id_type[id_type.len() - 5..id_type.len()];
    EGG_SALE_COUNT.update::<_, StdError>(deps.storage, |id| Ok(id.add(Uint64::new(1))))?;
    TOTAL_EGGS.update::<_, StdError>(deps.storage, |id| Ok(id.add(Uint64::new(1))))?;

    let msg = generate_egg_mint_msg(id, info.clone().sender.to_string())?;
    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(Execute {
            contract_addr: contracts.egg.clone(),
            msg: to_binary(&(msg))?,
            funds: vec![],
        }))
        .add_messages(vec![payment]))
}

pub fn execute_dragon_birth(
    deps: DepsMut,
    info: MessageInfo,
    _id: String,
    owner: String,
) -> Result<Response, ContractError> {
    let contracts = CONTRACTS.load(deps.storage)?;

    if info.sender != contracts.dragon {
        return Err(Unauthorized {
            msg: "only dragon contract can execute dragon birth ".to_string(),
        });
    }

    let total = TOTAL_EGGS.update::<_, StdError>(deps.storage, |id| Ok(id.add(Uint64::new(1))))?;
    let egg_no = total + Uint64::new(1);
    let egg_id = "00000".to_string() + &*egg_no.to_string();

    let msg = generate_egg_mint_msg(&*egg_id.to_string(), owner)?;
    Ok(Response::new().add_message(CosmosMsg::Wasm(Execute {
        contract_addr: contracts.egg,
        msg: to_binary(&(msg))?,
        funds: vec![],
    })))
}

pub fn execute_edit_state(
    deps: DepsMut,
    info: MessageInfo,
    base_price: Uint128,
    random_key: i32,
    hatch_price: Uint128,
    egg_sale_size: Uint64,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.owner {
        return Err(ContractError::Unauthorized {
            msg: "state can only be edited by the owner".to_string(),
        });
    }
    let new = State {
        owner: state.owner,
        base_price,
        hatch_price,
        random_key,
        egg_sale_size,
    };
    STATE.save(deps.storage, &new)?;
    Ok(Response::new())
}

pub fn execute_edit_contracts(
    deps: DepsMut,
    info: MessageInfo,
    egg: String,
    dragon: String,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage)?;
    if info.sender != state.owner {
        return Err(ContractError::Unauthorized {
            msg: "state can only be edited by the owner".to_string(),
        });
    }
    let new = ContractAddressList { egg, dragon };

    CONTRACTS.save(deps.storage, &new)?;
    Ok(Response::new())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetState {} => to_binary(&query_state(deps)?),
        QueryMsg::GetEggsaleOwnedCount {} => to_binary(&query_eggsale(deps)?),
    }
}

fn query_state(deps: Deps) -> StdResult<GetStateResponse> {
    let state = STATE.load(deps.storage)?;
    let contract = CONTRACTS.load(deps.storage)?;
    let total_eggs = TOTAL_EGGS.load(deps.storage)?;
    Ok(GetStateResponse {
        owner: state.owner,
        base_price: state.base_price,
        random_key: state.random_key,
        hatch_price: state.hatch_price,
        total_eggs,
        egg_sale_size: state.egg_sale_size,
        egg_contract: contract.egg,
        dragon_contract: contract.dragon,
    })
}

fn query_eggsale(deps: Deps) -> StdResult<GetEggsaleInfoResponse> {
    let state = STATE.load(deps.storage)?;
    let owned = EGG_SALE_COUNT.load(deps.storage)?;

    Ok(GetEggsaleInfoResponse {
        owned,
        size: state.egg_sale_size,
        base_price: state.base_price,
    })
}
