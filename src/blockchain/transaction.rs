#![allow(dead_code)]

use super::client::SorobanRpcClient;
use super::errors::BlockchainError;
use super::signer::SorobanSigner;
use super::types::{EscrowState, ExecutionResult, TransactionStatus};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use stellar_strkey::{ed25519, Contract, Strkey};
use stellar_xdr::curr::{
    AccountId, ExtensionPoint, Hash, HostFunction, InvokeContractArgs, LedgerKey,
    LedgerKeyContractData, Limits, Memo, MuxedAccount, Operation, OperationBody, Preconditions,
    PublicKey, ReadXdr, ScAddress, ScSymbol, ScVal, SequenceNumber, SorobanAddressCredentials,
    SorobanAuthorizationEntry, SorobanCredentials, SorobanTransactionData, Transaction,
    TransactionEnvelope, TransactionExt, TransactionSignaturePayload,
    TransactionSignaturePayloadTaggedTransaction, TransactionV1Envelope, Uint256, WriteXdr,
};
use tracing::{debug, info, warn};

#[derive(Debug)]
pub struct TransactionExecutor {
    rpc_client: SorobanRpcClient,
    contract_id: String,
    network_passphrase: String,
    service_signer: Option<SorobanSigner>,
    max_fee: u64,
    cached_sequence: Arc<AtomicI64>,
}

impl TransactionExecutor {
    pub fn new(
        rpc_client: SorobanRpcClient,
        contract_id: String,
        network_passphrase: String,
        service_signer: Option<SorobanSigner>,
        max_fee: u64,
    ) -> Self {
        Self {
            rpc_client,
            contract_id,
            network_passphrase,
            service_signer,
            max_fee,
            cached_sequence: Arc::new(AtomicI64::new(0)),
        }
    }

    pub fn contract_id(&self) -> &str {
        &self.contract_id
    }

    pub fn service_signer(&self) -> Result<&SorobanSigner, BlockchainError> {
        self.service_signer.as_ref().ok_or_else(|| {
            BlockchainError::InvalidConfig(
                "SOROBAN_SERVICE_SECRET_KEY is not configured in backend".to_string(),
            )
        })
    }

    pub fn compute_network_id(&self) -> Hash {
        let mut hasher = Sha256::new();
        hasher.update(self.network_passphrase.as_bytes());
        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result);
        Hash(bytes)
    }

    pub async fn get_next_sequence(&self, pubkey_str: &str) -> Result<i64, BlockchainError> {
        let current = self.cached_sequence.load(Ordering::SeqCst);
        if current > 0 {
            let next = self.cached_sequence.fetch_add(1, Ordering::SeqCst) + 1;
            return Ok(next);
        }

        let fetched_seq = self.rpc_client.get_account_sequence(pubkey_str).await?;
        let next = fetched_seq + 1;
        self.cached_sequence.store(next, Ordering::SeqCst);
        Ok(next)
    }

    pub fn parse_contract_address(contract_id: &str) -> Result<ScAddress, BlockchainError> {
        let decoded = Contract::from_string(contract_id.trim()).map_err(|e| {
            BlockchainError::InvalidConfig(format!("Invalid contract ID {}: {}", contract_id, e))
        })?;
        Ok(ScAddress::Contract(Hash(decoded.0)))
    }

    pub fn parse_account_address(account_id: &str) -> Result<ScAddress, BlockchainError> {
        let strkey = Strkey::from_string(account_id.trim()).map_err(|e| {
            BlockchainError::InvalidConfig(format!(
                "Invalid account public key {}: {}",
                account_id, e
            ))
        })?;

        let bytes = match strkey {
            Strkey::PublicKeyEd25519(pk) => pk.0,
            _ => {
                return Err(BlockchainError::InvalidConfig(
                    "Strkey is not an Ed25519 public key".to_string(),
                ))
            }
        };

        Ok(ScAddress::Account(AccountId(
            PublicKey::PublicKeyTypeEd25519(Uint256(bytes)),
        )))
    }

    pub fn build_invocation_op(
        contract_address: ScAddress,
        function_name: &str,
        args: Vec<ScVal>,
        auth_entries: Vec<SorobanAuthorizationEntry>,
    ) -> Result<Operation, BlockchainError> {
        let sym =
            ScSymbol(function_name.try_into().map_err(|_| {
                BlockchainError::InvalidConfig("Function name too long".to_string())
            })?);

        let invoke_args = InvokeContractArgs {
            contract_address,
            function_name: sym,
            args: args
                .try_into()
                .map_err(|_| BlockchainError::InvalidConfig("Too many args".to_string()))?,
        };

        let host_fn = HostFunction::InvokeContract(invoke_args);
        let host_fn_op = stellar_xdr::curr::InvokeHostFunctionOp {
            host_function: host_fn,
            auth: auth_entries
                .try_into()
                .map_err(|_| BlockchainError::InvalidConfig("Too many auth entries".to_string()))?,
        };

        Ok(Operation {
            source_account: None,
            body: OperationBody::InvokeHostFunction(host_fn_op),
        })
    }

    pub async fn execute_contract_call(
        &self,
        function_name: &str,
        args: Vec<ScVal>,
        escrow_id: u64,
        expected_amount: Option<f64>,
        auth_signer: Option<&SorobanSigner>,
    ) -> Result<ExecutionResult, BlockchainError> {
        let service_signer = self.service_signer()?;
        let contract_addr = Self::parse_contract_address(&self.contract_id)?;

        let op = Self::build_invocation_op(
            contract_addr.clone(),
            function_name,
            args.clone(),
            Vec::new(),
        )?;

        let service_pubkey = service_signer.public_key();
        let service_acc = Self::parse_account_address(service_pubkey)?;
        let service_account_id = match service_acc {
            ScAddress::Account(acc_id) => acc_id,
            _ => unreachable!(),
        };

        let muxed_account = match service_account_id.0 {
            PublicKey::PublicKeyTypeEd25519(u256) => MuxedAccount::Ed25519(u256),
        };

        let seq_num = self.get_next_sequence(service_pubkey).await?;

        let dummy_tx = Transaction {
            source_account: muxed_account.clone(),
            fee: 100,
            seq_num: SequenceNumber(seq_num),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![op.clone()]
                .try_into()
                .map_err(|_| BlockchainError::InvalidConfig("Too many operations".to_string()))?,
            ext: TransactionExt::V0,
        };

        let dummy_envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: dummy_tx,
            signatures: Vec::new().try_into().unwrap(),
        });

        let dummy_xdr = BASE64.encode(
            dummy_envelope
                .to_xdr(Limits::none())
                .map_err(|e| BlockchainError::InvalidResponse(e.to_string()))?,
        );

        let sim_res = self.rpc_client.simulate_transaction(&dummy_xdr).await?;
        if let Some(err) = sim_res.error {
            return Err(BlockchainError::TransactionRejected(format!(
                "Simulation failed: {}",
                err
            )));
        }

        let resource_fee: u32 = sim_res
            .min_resource_fee
            .as_deref()
            .unwrap_or("1000")
            .parse()
            .unwrap_or(1000);

        let total_fee = (resource_fee as u64) + 1000;
        if total_fee > self.max_fee {
            return Err(BlockchainError::TransactionRejected(format!(
                "Resource fee {} exceeds max allowed limit {}",
                total_fee, self.max_fee
            )));
        }

        let soroban_data = if let Some(data_b64) = sim_res.transaction_data.as_deref() {
            let data_bytes = BASE64.decode(data_b64).map_err(|e| {
                BlockchainError::InvalidResponse(format!("Invalid simulation data base64: {}", e))
            })?;
            SorobanTransactionData::from_xdr(&data_bytes, Limits::none()).map_err(|e| {
                BlockchainError::InvalidResponse(format!("Invalid SorobanTransactionData: {}", e))
            })?
        } else {
            SorobanTransactionData {
                ext: ExtensionPoint::V0,
                resources: stellar_xdr::curr::SorobanResources {
                    footprint: stellar_xdr::curr::LedgerFootprint {
                        read_only: Vec::new().try_into().unwrap(),
                        read_write: Vec::new().try_into().unwrap(),
                    },
                    instructions: 100_000,
                    read_bytes: 1000,
                    write_bytes: 1000,
                },
                resource_fee: resource_fee as i64,
            }
        };

        let mut final_auth_entries = Vec::new();
        if let Some(results) = sim_res.results {
            for res_item in results {
                if let Some(auth_b64_vec) = res_item.auth {
                    for auth_str in auth_b64_vec {
                        let auth_bytes = BASE64.decode(&auth_str).map_err(|e| {
                            BlockchainError::InvalidResponse(format!("Invalid auth base64: {}", e))
                        })?;
                        let mut entry =
                            SorobanAuthorizationEntry::from_xdr(&auth_bytes, Limits::none())
                                .map_err(|e| {
                                    BlockchainError::InvalidResponse(format!(
                                        "Invalid SorobanAuthorizationEntry: {}",
                                        e
                                    ))
                                })?;

                        if let Some(signer) = auth_signer {
                            let signer_pubkey = signer.public_key();
                            let signer_acc = Self::parse_account_address(signer_pubkey)?;

                            if entry.credentials.address().is_some() {
                                let nonce = entry.credentials.nonce().copied().unwrap_or(0);

                                let sig_payload = Hash([0u8; 32]);
                                let sig_bytes = signer.sign_payload(&sig_payload.0);

                                let sig_sc_val =
                                    ScVal::Bytes(sig_bytes.to_vec().try_into().map_err(|_| {
                                        BlockchainError::InvalidConfig(
                                            "Sig byte copy error".to_string(),
                                        )
                                    })?);

                                entry.credentials =
                                    SorobanCredentials::Address(SorobanAddressCredentials {
                                        address: signer_acc,
                                        nonce,
                                        signature_expiration_ledger: 1000,
                                        signature: sig_sc_val,
                                    });
                            }
                        }

                        final_auth_entries.push(entry);
                    }
                }
            }
        }

        let final_op = Self::build_invocation_op(
            contract_addr,
            function_name,
            args.clone(),
            final_auth_entries,
        )?;

        let final_tx = Transaction {
            source_account: muxed_account.clone(),
            fee: total_fee as u32,
            seq_num: SequenceNumber(seq_num),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![final_op]
                .try_into()
                .map_err(|_| BlockchainError::InvalidConfig("Too many ops".to_string()))?,
            ext: TransactionExt::V1(soroban_data),
        };

        let network_id = self.compute_network_id();
        let payload = TransactionSignaturePayload {
            network_id,
            tagged_transaction: TransactionSignaturePayloadTaggedTransaction::Tx(final_tx.clone()),
        };

        let payload_bytes = payload
            .to_xdr(Limits::none())
            .map_err(|e| BlockchainError::InvalidResponse(e.to_string()))?;

        let mut hasher = Sha256::new();
        hasher.update(&payload_bytes);
        let payload_hash = hasher.finalize();

        let sig_bytes = service_signer.sign_payload(&payload_hash);
        let mut hint_bytes = [0u8; 4];
        hint_bytes.copy_from_slice(&service_signer.public_key_bytes()[28..32]);

        let decorated_sig = stellar_xdr::curr::DecoratedSignature {
            hint: stellar_xdr::curr::SignatureHint(hint_bytes),
            signature: stellar_xdr::curr::Signature(sig_bytes.to_vec().try_into().map_err(
                |_| BlockchainError::InvalidConfig("Decorated signature fail".to_string()),
            )?),
        };

        let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
            tx: final_tx,
            signatures: vec![decorated_sig]
                .try_into()
                .map_err(|_| BlockchainError::InvalidConfig("Too many signatures".to_string()))?,
        });

        let envelope_xdr = BASE64.encode(
            envelope
                .to_xdr(Limits::none())
                .map_err(|e| BlockchainError::InvalidResponse(e.to_string()))?,
        );

        let send_res = self.rpc_client.send_transaction(&envelope_xdr).await?;
        let hash = send_res.hash;

        let status = self
            .rpc_client
            .poll_transaction_confirmation(&hash, 15, 1000)
            .await?;

        if status != TransactionStatus::Success {
            return Err(BlockchainError::TransactionFailed(format!(
                "Transaction {} failed with status {:?}",
                hash, status
            )));
        }

        let (ver_status, semantics) = self.rpc_client.get_transaction_semantics(&hash).await?;
        if ver_status != TransactionStatus::Success {
            return Err(BlockchainError::TransactionFailed(
                "Transaction verification status is not SUCCESS".to_string(),
            ));
        }

        if let Some(parsed_func) = &semantics.function {
            if parsed_func != function_name {
                return Err(BlockchainError::SemanticMismatch(format!(
                    "Function mismatch: expected {}, found {}",
                    function_name, parsed_func
                )));
            }
        }

        let final_state = match function_name {
            "create_escrow" => Some(EscrowState::Created),
            "fund_escrow" => Some(EscrowState::Funded),
            "release_escrow" => Some(EscrowState::Released),
            "refund_escrow" => Some(EscrowState::Refunded),
            "dispute_escrow" => Some(EscrowState::Disputed),
            "resolve_dispute" => {
                if args.get(1) == Some(&ScVal::Bool(true)) {
                    Some(EscrowState::Released)
                } else {
                    Some(EscrowState::Refunded)
                }
            }
            _ => None,
        };

        Ok(ExecutionResult {
            hash,
            status: TransactionStatus::Success,
            contract_id: self.contract_id.clone(),
            function: function_name.to_string(),
            escrow_id,
            amount: expected_amount.or(semantics.amount),
            token: None,
            final_escrow_state: final_state,
        })
    }
}

trait CredentialsExt {
    fn address(&self) -> Option<&ScAddress>;
    fn nonce(&self) -> Option<&i64>;
}

impl CredentialsExt for SorobanCredentials {
    fn address(&self) -> Option<&ScAddress> {
        match self {
            SorobanCredentials::Address(c) => Some(&c.address),
            _ => None,
        }
    }
    fn nonce(&self) -> Option<&i64> {
        match self {
            SorobanCredentials::Address(c) => Some(&c.nonce),
            _ => None,
        }
    }
}
