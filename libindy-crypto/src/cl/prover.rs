use bn::BigNumber;
use cl::*;
use cl::constants::*;
use errors::IndyCryptoError;
use pair::*;
use super::helpers::*;
use utils::commitment::{get_pedersen_commitment, get_exponentiated_generators};

use std::collections::{BTreeMap, HashSet};
use std::iter::FromIterator;

/// Credentials owner that can proof and partially disclose the credentials to verifier.
pub struct Prover {}

impl Prover {
    /// Creates a master secret.
    ///
    /// # Example
    /// ```
    /// use indy_crypto::cl::prover::Prover;
    ///
    /// let _master_secret = Prover::new_master_secret().unwrap();
    /// ```
    pub fn new_master_secret() -> Result<MasterSecret, IndyCryptoError> {
        Ok(MasterSecret {
            ms: bn_rand(LARGE_MASTER_SECRET)?
        })
    }

    /// Creates blinded master secret for given issuer key and master secret.
    ///
    /// # Arguments
    /// * `credential_pub_key` - Credential public keys.
    /// * `credential_key_correctness_proof` - Credential key correctness proof.
    /// * `master_secret` - Master secret.
    /// * `master_secret_blinding_nonce` - Nonce used for creation of blinded_master_secret_correctness_proof.
    ///
    /// # Example
    /// ```
    /// use indy_crypto::cl::new_nonce;
    /// use indy_crypto::cl::issuer::Issuer;
    /// use indy_crypto::cl::prover::Prover;
    /// 
    /// let mut credential_schema_builder = Issuer::new_credential_schema_builder().unwrap();
    /// credential_schema_builder.add_attr("sex").unwrap();
    /// let credential_schema = credential_schema_builder.finalize().unwrap();
    ///
    /// let (credential_pub_key, _credential_priv_key, cred_key_correctness_proof) = Issuer::new_credential_def(&credential_schema, false).unwrap();
    ///
    /// let master_secret = Prover::new_master_secret().unwrap();
    /// let master_secret_blinding_nonce = new_nonce().unwrap();
    /// let (_blinded_master_secret, _master_secret_blinding_data, _blinded_master_secret_correctness_proof) =
    ///     Prover::blind_credential_secrets(&credential_pub_key,
    ///                                 &cred_key_correctness_proof,
    ///                                 &master_secret,
    ///                                 &master_secret_blinding_nonce).unwrap();
    /// ```
    pub fn blind_credential_secrets(credential_pub_key: &CredentialPublicKey,
                                    credential_key_correctness_proof: &CredentialKeyCorrectnessProof,
                                    credential_values: &CredentialValues,
                                    credential_nonce: &Nonce) -> Result<(BlindedCredentialSecrets,
                                                                         CredentialSecretsBlindingFactors,
                                                                         BlindedCredentialSecretsCorrectnessProof), IndyCryptoError> {
        trace!("Prover::blind_credential_secrets: >>> credential_pub_key: {:?}, \
                                                      credential_key_correctness_proof: {:?}, \
                                                      credential_values: {:?}, \
                                                      credential_nonce: {:?}",
                                                      credential_pub_key,
                                                      credential_key_correctness_proof,
                                                      credential_values,
                                                      credential_nonce);

        Prover::_check_credential_key_correctness_proof(&credential_pub_key.p_key, credential_key_correctness_proof)?;

        let primary_blinded_credential_secrets =
            Prover::_generate_primary_blinded_credential_secrets(&credential_pub_key.p_key, &credential_values)?;

        let blinded_revocation_master_secret = match credential_pub_key.r_key {
            Some(ref r_pk) => Some(Prover::_generate_revocation_blinded_credential_secrets(r_pk)?),
            _ => None
        };

        let blinded_credential_secrets_correctness_proof =
            Prover::_new_blinded_credential_secrets_correctness_proof(&credential_pub_key.p_key,
                                                                      &primary_blinded_credential_secrets,
                                                                      &credential_nonce,
                                                                      &credential_values)?;

        let blinded_credential_secrets = BlindedCredentialSecrets {
            u: primary_blinded_credential_secrets.u,
            ur: blinded_revocation_master_secret.as_ref().map(|d| d.ur),
            committed_attributes: primary_blinded_credential_secrets.committed_attributes
        };

        let credential_secrets_blinding_factors = CredentialSecretsBlindingFactors {
            v_prime: primary_blinded_credential_secrets.v_prime,
            vr_prime: blinded_revocation_master_secret.map(|d| d.vr_prime)
        };

        trace!("Prover::blind_credential_secrets: <<< blinded_credential_secrets: {:?}, \
                                                      credential_secrets_blinding_factors: {:?}, \
                                                      blinded_credential_secrets_correctness_proof: {:?},",
                                                      blinded_credential_secrets,
                                                      credential_secrets_blinding_factors,
                                                      blinded_credential_secrets_correctness_proof);

        Ok((blinded_credential_secrets, credential_secrets_blinding_factors, blinded_credential_secrets_correctness_proof))
    }

    /// Updates the credential signature by a master secret blinding data.
    ///
    /// # Arguments
    /// * `credential_signature` - Credential signature generated by Issuer.
    /// * `credential_values` - Credential values.
    /// * `signature_correctness_proof` - Credential signature correctness proof.
    /// * `master_secret_blinding_data` - Master secret blinding data.
    /// * `master_secret` - Master secret.
    /// * `credential_pub_key` - Credential public key.
    /// * `nonce` -  Nonce was used by Issuer for the creation of signature_correctness_proof.
    /// * `rev_key_pub` - (Optional) Revocation registry public key.
    /// * `rev_reg` - (Optional) Revocation registry.
    /// * `witness` - (Optional) Witness.
    ///
    /// # Example
    /// ```
    /// use indy_crypto::cl::new_nonce;
    /// use indy_crypto::cl::issuer::Issuer;
    /// use indy_crypto::cl::prover::Prover;
    ///
    /// let mut credential_schema_builder = Issuer::new_credential_schema_builder().unwrap();
    /// credential_schema_builder.add_attr("sex").unwrap();
    /// let credential_schema = credential_schema_builder.finalize().unwrap();
    ///
    /// let (credential_pub_key, credential_priv_key, cred_key_correctness_proof) = Issuer::new_credential_def(&credential_schema, false).unwrap();
    ///
    /// let master_secret = Prover::new_master_secret().unwrap();
    /// let master_secret_blinding_nonce = new_nonce().unwrap();
    /// let (blinded_master_secret, master_secret_blinding_data, blinded_master_secret_correctness_proof) =
    ///     Prover::blind_master_secret(&credential_pub_key, &cred_key_correctness_proof, &master_secret, &master_secret_blinding_nonce).unwrap();
    ///
    /// let mut credential_values_builder = Issuer::new_credential_values_builder().unwrap();
    /// credential_values_builder.add_value("sex", "5944657099558967239210949258394887428692050081607692519917050011144233115103").unwrap();
    /// let credential_values = credential_values_builder.finalize().unwrap();
    ///
    /// let credential_issuance_nonce = new_nonce().unwrap();
    ///
    /// let (mut credential_signature, signature_correctness_proof) =
    ///     Issuer::sign_credential("CnEDk9HrMnmiHXEV1WFgbVCRteYnPqsJwrTdcZaNhFVW",
    ///                             &blinded_master_secret,
    ///                             &blinded_master_secret_correctness_proof,
    ///                             &master_secret_blinding_nonce,
    ///                             &credential_issuance_nonce,
    ///                             &credential_values,
    ///                             &credential_pub_key,
    ///                             &credential_priv_key).unwrap();
    ///
    /// Prover::process_credential_signature(&mut credential_signature,
    ///                                      &credential_values,
    ///                                      &signature_correctness_proof,
    ///                                      &master_secret_blinding_data,
    ///                                      &master_secret,
    ///                                      &credential_pub_key,
    ///                                      &credential_issuance_nonce,
    ///                                      None, None, None).unwrap();
    /// ```
    pub fn process_credential_signature(credential_signature: &mut CredentialSignature,
                                        credential_values: &CredentialValues,
                                        signature_correctness_proof: &SignatureCorrectnessProof,
                                        credential_secrets_blinding_factors: &CredentialSecretsBlindingFactors,
                                        credential_pub_key: &CredentialPublicKey,
                                        nonce: &Nonce,
                                        rev_key_pub: Option<&RevocationKeyPublic>,
                                        rev_reg: Option<&RevocationRegistry>,
                                        witness: Option<&Witness>) -> Result<(), IndyCryptoError> {
        trace!("Prover::process_credential_signature: >>> credential_signature: {:?}, \
                                                          credential_values: {:?}, \
                                                          signature_correctness_proof: {:?}, \
                                                          credential_secrets_blinding_factors: {:?}, \
                                                          credential_pub_key: {:?}, \
                                                          nonce: {:?}, \
                                                          rev_key_pub: {:?}, \
                                                          rev_reg: {:?}, \
                                                          witness: {:?}",
                                                        credential_signature,
                                                        credential_values,
                                                        signature_correctness_proof,
                                                        credential_secrets_blinding_factors,
                                                        credential_pub_key,
                                                        nonce,
                                                        rev_key_pub,
                                                        rev_reg,
                                                        witness);

        Prover::_process_primary_credential(&mut credential_signature.p_credential, &credential_secrets_blinding_factors.v_prime)?;

        Prover::_check_signature_correctness_proof(&credential_signature.p_credential,
                                                   credential_values,
                                                   signature_correctness_proof,
                                                   &credential_pub_key.p_key,
                                                   nonce)?;

        if let (&mut Some(ref mut non_revocation_cred), Some(ref vr_prime), &Some(ref r_key),
            Some(ref r_key_pub), Some(ref r_reg), Some(ref witness)) = (&mut credential_signature.r_credential,
                                                                        credential_secrets_blinding_factors.vr_prime,
                                                                        &credential_pub_key.r_key,
                                                                        rev_key_pub,
                                                                        rev_reg,
                                                                        witness) {
            Prover::_process_non_revocation_credential(non_revocation_cred,
                                                       vr_prime,
                                                       &r_key,
                                                       r_key_pub,
                                                       r_reg,
                                                       witness)?;
        }

        trace!("Prover::process_credential_signature: <<<");

        Ok(())
    }

    /// Creates and returns proof builder.
    ///
    /// The purpose of proof builder is building of proof entity according to the given request .
    /// # Example
    /// ```
    /// use indy_crypto::cl::prover::Prover;
    ///
    /// let _proof_builder = Prover::new_proof_builder();
    pub fn new_proof_builder() -> Result<ProofBuilder, IndyCryptoError> {
        Ok(ProofBuilder {
            init_proofs: BTreeMap::new(),
            c_list: Vec::new(),
            tau_list: Vec::new()
        })
    }

    fn _check_credential_key_correctness_proof(pr_pub_key: &CredentialPrimaryPublicKey,
                                               key_correctness_proof: &CredentialKeyCorrectnessProof) -> Result<(), IndyCryptoError> {
        trace!("Prover::_check_credential_key_correctness_proof: >>> pr_pub_key: {:?}, key_correctness_proof: {:?}",
               pr_pub_key, key_correctness_proof);

        let mut ctx = BigNumber::new_context()?;

        let z_inverse = pr_pub_key.z.inverse(&pr_pub_key.n, Some(&mut ctx))?;
        let z_cap = get_pedersen_commitment(&z_inverse, &key_correctness_proof.c,
                                            &pr_pub_key.s, &key_correctness_proof.xz_cap, &pr_pub_key.n, &mut ctx)?;

        let mut r_cap: BTreeMap<String, BigNumber> = BTreeMap::new();
        for (key, r_value) in pr_pub_key.r.iter() {
            let xr_cap_value = key_correctness_proof.xr_cap
                .get(key)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in key_correctness_proof.xr_cap", key)))?;

            let r_inverse = r_value.inverse(&pr_pub_key.n, Some(&mut ctx))?;
            let val = get_pedersen_commitment(&r_inverse, &key_correctness_proof.c,
                                              &pr_pub_key.s, &xr_cap_value, &pr_pub_key.n, &mut ctx)?;

            r_cap.insert(key.to_owned(), val);
        }

        let mut values: Vec<u8> = Vec::new();
        values.extend_from_slice(&pr_pub_key.z.to_bytes()?);
        for val in pr_pub_key.r.values() {
            values.extend_from_slice(&val.to_bytes()?);
        }
        values.extend_from_slice(&z_cap.to_bytes()?);
        for val in r_cap.values() {
            values.extend_from_slice(&val.to_bytes()?);
        }

        let c = get_hash_as_int(&mut vec![values])?;

        let valid = key_correctness_proof.c.eq(&c);

        if !valid {
            return Err(IndyCryptoError::InvalidStructure(format!("Invalid Credential key correctness proof")));
        }

        trace!("Prover::_check_credential_key_correctness_proof: <<<");

        Ok(())
    }

    fn _generate_primary_blinded_credential_secrets(p_pub_key: &CredentialPrimaryPublicKey,
                                                    credential_values: &CredentialValues) -> Result<PrimaryBlindedCredentialSecretsFactors, IndyCryptoError> {
        trace!("Prover::_generate_blinded_primary_master_secret: >>> p_pub_key: {:?}, credential_values: {:?}", p_pub_key, credential_values);

        let mut ctx = BigNumber::new_context()?;
        let v_prime = bn_rand(LARGE_VPRIME)?;

        let mut u = p_pub_key.s.mod_exp(&v_prime, &p_pub_key.n, Some(&mut ctx))?;

        let mut committed_attributes = BTreeMap::new();

        for (key, value) in credential_values.attrs_values.iter().filter(|&(_, v)| v.blinding_factor.is_some()) {
            let kc = key.clone();
            let pk_r = p_pub_key.r
                .get(key)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in pk.r", key)))?;

            u = u.mod_mul(&pk_r.mod_exp(&value.value, &p_pub_key.n, Some(&mut ctx))?,
                          &p_pub_key.n, Some(&mut ctx))?;

            let bf = value.blinding_factor
                          .as_ref()
                          .ok_or(IndyCryptoError::InvalidStructure(format!("Blinding Factor by key '{}' does not contain a value in credential_values.attrs_values", key)))?;

            committed_attributes.insert(kc, get_pedersen_commitment(&p_pub_key.s, &bf,
                                                                    &p_pub_key.z, &value.value,
                                                                    &p_pub_key.n, &mut ctx)?);
        }

        let primary_blinded_cred_secrets = PrimaryBlindedCredentialSecretsFactors { u, v_prime, committed_attributes };

        trace!("Prover::_generate_blinded_primary_master_secret: <<< primary_blinded_cred_secrets: {:?}", primary_blinded_cred_secrets);

        Ok(primary_blinded_cred_secrets)
    }

    fn _generate_revocation_blinded_credential_secrets(r_pub_key: &CredentialRevocationPublicKey) -> Result<RevocationBlindedCredentialSecretsFactors, IndyCryptoError> {
        trace!("Prover::_generate_revocation_blinded_credential_secrets: >>> r_pub_key: {:?}", r_pub_key);

        let vr_prime = GroupOrderElement::new()?;
        let ur = r_pub_key.h2.mul(&vr_prime)?;

        let revocation_blinded_cred_secrets = RevocationBlindedCredentialSecretsFactors { ur, vr_prime };

        trace!("Prover::_generate_revocation_blinded_credential_secrets: <<< revocation_blinded_cred_secrets: {:?}", revocation_blinded_cred_secrets);

        Ok(revocation_blinded_cred_secrets)
    }

    fn _new_blinded_credential_secrets_correctness_proof(p_pub_key: &CredentialPrimaryPublicKey,
                                                         primary_blinded_cred_secrets: &PrimaryBlindedCredentialSecretsFactors,
                                                         nonce: &BigNumber,
                                                         cred_values: &CredentialValues) -> Result<BlindedCredentialSecretsCorrectnessProof, IndyCryptoError> {
        trace!("Prover::_new_blinded_credential_secrets_correctness_proof: >>> p_pub_key: {:?}, primary_blinded_cred_secrets: {:?}, nonce: {:?}, cred_values: {:?}",
               primary_blinded_cred_secrets, nonce, p_pub_key, cred_values);

        let mut ctx = BigNumber::new_context()?;

        let v_dash_tilde = bn_rand(LARGE_VPRIME_TILDE)?;

        let mut u_tilde = p_pub_key.s.mod_exp(&v_dash_tilde, &p_pub_key.n, Some(&mut ctx))?;
        let mut m_tildes = BTreeMap::new();
        let mut r_tildes = BTreeMap::new();

        let mut values: Vec<u8> = Vec::new();

        for (key, value) in &primary_blinded_cred_secrets.committed_attributes {
            let m_tilde = bn_rand(LARGE_MTILDE)?;
            let r_tilde = bn_rand(LARGE_MTILDE)?;

            let pk_r = p_pub_key.r
                .get(key)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in pk.r", key)))?;

            u_tilde = u_tilde.mod_mul(&pk_r.mod_exp(&m_tilde, &p_pub_key.n, Some(&mut ctx))?,
                                      &p_pub_key.n, Some(&mut ctx))?;

            let commitment_tilde = get_pedersen_commitment(&p_pub_key.z,
                                                           &m_tilde,
                                                           &p_pub_key.s,
                                                           &r_tilde,
                                                           &p_pub_key.n,
                                                           &mut ctx)?;
            m_tildes.insert(key.clone(), m_tilde);
            r_tildes.insert(key.clone(), r_tilde);

            values.extend_from_slice(&commitment_tilde.to_bytes()?);
            values.extend_from_slice(&value.to_bytes()?);
        }

        values.extend_from_slice(&primary_blinded_cred_secrets.u.to_bytes()?);
        values.extend_from_slice(&u_tilde.to_bytes()?);
        values.extend_from_slice(&nonce.to_bytes()?);

        let c = get_hash_as_int(&vec![values])?;

        let v_dash_cap = c.mul(&primary_blinded_cred_secrets.v_prime, Some(&mut ctx))?
                          .add(&v_dash_tilde)?;

        let mut m_caps = BTreeMap::new();
        let mut r_caps = BTreeMap::new();

        for key in m_tildes.keys() {
            let ca = cred_values.attrs_values
                      .get(key)
                      .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in cred_values.committed_attributes", key)))?;

            let bf = ca.blinding_factor
                       .as_ref()
                       .ok_or(IndyCryptoError::InvalidStructure(format!("Blinding Factor by key '{}' does not contain a value in cred_values.committed_attributes", key)))?;

            let m_cap = m_tildes[key].add(&c.mul(&ca.value, Some(&mut ctx))?)?;
            let r_cap = r_tildes[key].add(&c.mul(&bf, Some(&mut ctx))?)?;

            m_caps.insert(key.clone(), m_cap);
            r_caps.insert(key.clone(), r_cap);
        }


        let blinded_credential_secrets_correctness_proof = BlindedCredentialSecretsCorrectnessProof { c, v_dash_cap, m_caps, r_caps };

        trace!("Prover::_new_blinded_credential_secrets_correctness_proof: <<< blinded_primary_master_secret_correctness_proof: {:?}",
               blinded_credential_secrets_correctness_proof);

        Ok(blinded_credential_secrets_correctness_proof)
    }

    fn _process_primary_credential(p_cred: &mut PrimaryCredentialSignature,
                                   v_prime: &BigNumber) -> Result<(), IndyCryptoError> {
        trace!("Prover::_process_primary_credential: >>> p_cred: {:?}, v_prime: {:?}", p_cred, v_prime);

        p_cred.v = v_prime.add(&p_cred.v)?;

        trace!("Prover::_process_primary_credential: <<<");

        Ok(())
    }

    fn _process_non_revocation_credential(r_cred: &mut NonRevocationCredentialSignature,
                                          vr_prime: &GroupOrderElement,
                                          cred_rev_pub_key: &CredentialRevocationPublicKey,
                                          rev_key_pub: &RevocationKeyPublic,
                                          rev_reg: &RevocationRegistry,
                                          witness: &Witness) -> Result<(), IndyCryptoError> {
        trace!("Prover::_process_non_revocation_credential: >>> r_cred: {:?}, vr_prime: {:?}, cred_rev_pub_key: {:?}, rev_reg: {:?}, rev_key_pub: {:?}",
               r_cred, vr_prime, cred_rev_pub_key, rev_reg, rev_key_pub);

        let r_cnxt_m2 = BigNumber::from_bytes(&r_cred.m2.to_bytes()?)?;
        r_cred.vr_prime_prime = vr_prime.add_mod(&r_cred.vr_prime_prime)?;
        Prover::_test_witness_signature(&r_cred, cred_rev_pub_key, rev_key_pub, rev_reg, witness, &r_cnxt_m2)?;

        trace!("Prover::_process_non_revocation_credential: <<<");

        Ok(())
    }

    fn _check_signature_correctness_proof(p_cred_sig: &PrimaryCredentialSignature,
                                          cred_values: &CredentialValues,
                                          signature_correctness_proof: &SignatureCorrectnessProof,
                                          p_pub_key: &CredentialPrimaryPublicKey,
                                          nonce: &Nonce) -> Result<(), IndyCryptoError> {
        trace!("Prover::_check_signature_correctness_proof: >>> p_cred_sig: {:?}, \
                                                                cred_values: {:?}, \
                                                                signature_correctness_proof: {:?}, \
                                                                p_pub_key: {:?}, \
                                                                nonce: {:?}",
                                                                p_cred_sig,
                                                                cred_values,
                                                                signature_correctness_proof,
                                                                p_pub_key,
                                                                nonce);

        let mut ctx = BigNumber::new_context()?;

        if !p_cred_sig.e.is_prime(Some(&mut ctx))? {
            return Err(IndyCryptoError::InvalidStructure(format!("Invalid Signature correctness proof")));
        }

        let mut generators_and_exponents = Vec::new();
        generators_and_exponents.push((&p_pub_key.s, &p_cred_sig.v));
        generators_and_exponents.push((&p_pub_key.rctxt, &p_cred_sig.m_2));

        for (key, attr) in cred_values.attrs_values.iter() {
            let pk_r = p_pub_key.r
                .get(key)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in pk.r", key)))?;

            generators_and_exponents.push((&pk_r, &attr.value));
        }

        let rx = get_exponentiated_generators(generators_and_exponents, &p_pub_key.n, &mut ctx)?;

        let q = p_pub_key.z.mod_div(&rx, &p_pub_key.n)?;

        let expected_q = p_cred_sig.a.mod_exp(&p_cred_sig.e, &p_pub_key.n, Some(&mut ctx))?;

        if !q.eq(&expected_q) {
            return Err(IndyCryptoError::InvalidStructure(format!("Invalid Signature correctness proof")));
        }

        let degree = signature_correctness_proof.c.add(
            &signature_correctness_proof.se.mul(&p_cred_sig.e, Some(&mut ctx))?
        )?;

        let a_cap = p_cred_sig.a.mod_exp(&degree, &p_pub_key.n, Some(&mut ctx))?;

        let mut values: Vec<u8> = Vec::new();
        values.extend_from_slice(&q.to_bytes()?);
        values.extend_from_slice(&p_cred_sig.a.to_bytes()?);
        values.extend_from_slice(&a_cap.to_bytes()?);
        values.extend_from_slice(&nonce.to_bytes()?);

        let c = get_hash_as_int(&vec![values])?;

        let valid = signature_correctness_proof.c.eq(&c);

        if !valid {
            return Err(IndyCryptoError::InvalidStructure(format!("Invalid Signature correctness proof")));
        }

        trace!("Prover::_check_signature_correctness_proof: <<<");

        Ok(())
    }

    fn _test_witness_signature(r_cred: &NonRevocationCredentialSignature,
                               cred_rev_pub_key: &CredentialRevocationPublicKey,
                               rev_key_pub: &RevocationKeyPublic,
                               rev_reg: &RevocationRegistry,
                               witness: &Witness,
                               r_cnxt_m2: &BigNumber) -> Result<(), IndyCryptoError> {
        trace!("Prover::_test_witness_signature: >>> r_cred: {:?}, cred_rev_pub_key: {:?}, rev_key_pub: {:?}, rev_reg: {:?}, r_cnxt_m2: {:?}",
               r_cred, cred_rev_pub_key, rev_key_pub, rev_reg, r_cnxt_m2);

        let z_calc = Pair::pair(&r_cred.witness_signature.g_i, &rev_reg.accum)?
            .mul(&Pair::pair(&cred_rev_pub_key.g, &witness.omega)?.inverse()?)?;

        if z_calc != rev_key_pub.z {
            return Err(IndyCryptoError::InvalidStructure("Issuer is sending incorrect data".to_string()));
        }
        let pair_gg_calc = Pair::pair(&cred_rev_pub_key.pk.add(&r_cred.g_i)?, &r_cred.witness_signature.sigma_i)?;
        let pair_gg = Pair::pair(&cred_rev_pub_key.g, &cred_rev_pub_key.g_dash)?;

        if pair_gg_calc != pair_gg {
            return Err(IndyCryptoError::InvalidStructure("Issuer is sending incorrect data".to_string()));
        }

        let m2 = GroupOrderElement::from_bytes(&r_cnxt_m2.to_bytes()?)?;

        let pair_h1 = Pair::pair(&r_cred.sigma, &cred_rev_pub_key.y.add(&cred_rev_pub_key.h_cap.mul(&r_cred.c)?)?)?;
        let pair_h2 = Pair::pair(
            &cred_rev_pub_key.h0
                .add(&cred_rev_pub_key.h1.mul(&m2)?)?
                .add(&cred_rev_pub_key.h2.mul(&r_cred.vr_prime_prime)?)?
                .add(&r_cred.g_i)?,
            &cred_rev_pub_key.h_cap
        )?;

        if pair_h1 != pair_h2 {
            return Err(IndyCryptoError::InvalidStructure("Issuer is sending incorrect data".to_string()));
        }

        trace!("Prover::_test_witness_signature: <<<");

        Ok(())
    }
}

#[derive(Debug)]
pub struct ProofBuilder {
    pub init_proofs: BTreeMap<String, InitProof>,
    pub c_list: Vec<Vec<u8>>,
    pub tau_list: Vec<Vec<u8>>,
}

impl ProofBuilder {
    /// Adds sub proof request to proof builder which will be used fo building of proof.
    /// Part of proof request related to a particular schema-key.
    ///
    /// # Arguments
    /// * `proof_builder` - Proof builder.
    /// * `key_id` - Unique credential identifier.
    /// * `sub_proof_request` -Requested attributes and predicates.
    /// * `credential_schema` - Credential schema.
    /// * `credential_signature` - Credential signature.
    /// * `credential_values` - Credential values.
    /// * `credential_pub_key` - Credential public key.
    /// * `rev_reg_pub` - (Optional) Revocation registry public.
    ///
    /// #Example
    /// ```
    /// use indy_crypto::cl::new_nonce;
    /// use indy_crypto::cl::issuer::Issuer;
    /// use indy_crypto::cl::prover::Prover;
    /// use indy_crypto::cl::verifier::Verifier;
    ///
    /// let mut credential_schema_builder = Issuer::new_credential_schema_builder().unwrap();
    /// credential_schema_builder.add_attr("sex").unwrap();
    /// let credential_schema = credential_schema_builder.finalize().unwrap();
    ///
    /// let (credential_pub_key, credential_priv_key, cred_key_correctness_proof) = Issuer::new_credential_def(&credential_schema, false).unwrap();
    ///
    /// let master_secret = Prover::new_master_secret().unwrap();
    /// let master_secret_blinding_nonce = new_nonce().unwrap();
    /// let (blinded_master_secret, master_secret_blinding_data, blinded_master_secret_correctness_proof) =
    ///     Prover::blind_master_secret(&credential_pub_key, &cred_key_correctness_proof, &master_secret, &master_secret_blinding_nonce).unwrap();
    ///
    /// let mut credential_values_builder = Issuer::new_credential_values_builder().unwrap();
    /// credential_values_builder.add_value("sex", "5944657099558967239210949258394887428692050081607692519917050011144233115103").unwrap();
    /// let credential_values = credential_values_builder.finalize().unwrap();
    ///
    /// let credential_issuance_nonce = new_nonce().unwrap();
    ///
    /// let (mut credential_signature, signature_correctness_proof) =
    ///     Issuer::sign_credential("CnEDk9HrMnmiHXEV1WFgbVCRteYnPqsJwrTdcZaNhFVW",
    ///                             &blinded_master_secret,
    ///                             &blinded_master_secret_correctness_proof,
    ///                             &master_secret_blinding_nonce,
    ///                             &credential_issuance_nonce,
    ///                             &credential_values,
    ///                             &credential_pub_key,
    ///                             &credential_priv_key).unwrap();
    ///
    /// Prover::process_credential_signature(&mut credential_signature,
    ///                                      &credential_values,
    ///                                      &signature_correctness_proof,
    ///                                      &master_secret_blinding_data,
    ///                                      &master_secret,
    ///                                      &credential_pub_key,
    ///                                      &credential_issuance_nonce,
    ///                                      None, None, None).unwrap();
    ///
    /// let mut sub_proof_request_builder = Verifier::new_sub_proof_request_builder().unwrap();
    /// sub_proof_request_builder.add_revealed_attr("sex").unwrap();
    /// let sub_proof_request = sub_proof_request_builder.finalize().unwrap();
    ///
    /// let mut proof_builder = Prover::new_proof_builder().unwrap();
    /// proof_builder.add_sub_proof_request("issuer_key_id_1",
    ///                                     &sub_proof_request,
    ///                                     &credential_schema,
    ///                                     &credential_signature,
    ///                                     &credential_values,
    ///                                     &credential_pub_key,
    ///                                     None,
    ///                                     None).unwrap();
    /// ```
    pub fn add_sub_proof_request(&mut self,
                                 key_id: &str,
                                 sub_proof_request: &SubProofRequest,
                                 credential_schema: &CredentialSchema,
                                 credential_signature: &CredentialSignature,
                                 credential_values: &CredentialValues,
                                 credential_pub_key: &CredentialPublicKey,
                                 rev_reg: Option<&RevocationRegistry>,
                                 witness: Option<&Witness>) -> Result<(), IndyCryptoError> {
        trace!("ProofBuilder::add_sub_proof_request: >>> key_id: {:?}, credential_signature: {:?}, credential_values: {:?}, credential_pub_key: {:?}, \
        rev_reg: {:?}, sub_proof_request: {:?}, credential_schema: {:?}",
               key_id, credential_signature, credential_values, credential_pub_key, rev_reg, sub_proof_request, credential_schema);

        ProofBuilder::_check_add_sub_proof_request_params_consistency(credential_values, sub_proof_request, credential_schema)?;

        let mut non_revoc_init_proof = None;
        let mut m2_tilde: Option<BigNumber> = None;

        if let (&Some(ref r_cred), &Some(ref r_reg), &Some(ref r_pub_key), &Some(ref witness)) = (&credential_signature.r_credential,
                                                                                                  &rev_reg,
                                                                                                  &credential_pub_key.r_key,
                                                                                                  &witness) {
            let proof = ProofBuilder::_init_non_revocation_proof(&r_cred,
                                                                 &r_reg,
                                                                 &r_pub_key,
                                                                 &witness)?;

            self.c_list.extend_from_slice(&proof.as_c_list()?);
            self.tau_list.extend_from_slice(&proof.as_tau_list()?);
            m2_tilde = Some(group_element_to_bignum(&proof.tau_list_params.m2)?);
            non_revoc_init_proof = Some(proof);
        }

        let primary_init_proof = ProofBuilder::_init_primary_proof(&credential_pub_key.p_key,
                                                                   &credential_signature.p_credential,
                                                                   &credential_values,
                                                                   &credential_schema,
                                                                   &sub_proof_request,
                                                                   m2_tilde)?;

        self.c_list.extend_from_slice(&primary_init_proof.as_c_list()?);
        self.tau_list.extend_from_slice(&primary_init_proof.as_tau_list()?);

        let init_proof = InitProof {
            primary_init_proof,
            non_revoc_init_proof,
            credential_values: credential_values.clone()?,
            sub_proof_request: sub_proof_request.clone(),
            credential_schema: credential_schema.clone()
        };
        self.init_proofs.insert(key_id.to_owned(), init_proof);

        trace!("ProofBuilder::add_sub_proof_request: <<<");

        Ok(())
    }

    /// Finalize proof.
    ///
    /// # Arguments
    /// * `proof_builder` - Proof builder.
    /// * `nonce` - Nonce.
    /// * `master_secret` - Master secret.
    ///
    /// #Example
    /// ```
    /// use indy_crypto::cl::new_nonce;
    /// use indy_crypto::cl::issuer::Issuer;
    /// use indy_crypto::cl::prover::Prover;
    /// use indy_crypto::cl::verifier::Verifier;
    ///
    /// let mut credential_schema_builder = Issuer::new_credential_schema_builder().unwrap();
    /// credential_schema_builder.add_attr("sex").unwrap();
    /// let credential_schema = credential_schema_builder.finalize().unwrap();
    ///
    /// let (credential_pub_key, credential_priv_key, cred_key_correctness_proof) = Issuer::new_credential_def(&credential_schema, false).unwrap();
    ///
    /// let master_secret = Prover::new_master_secret().unwrap();
    /// let master_secret_blinding_nonce = new_nonce().unwrap();
    /// let (blinded_master_secret, master_secret_blinding_data, blinded_master_secret_correctness_proof) =
    ///     Prover::blind_master_secret(&credential_pub_key, &cred_key_correctness_proof, &master_secret, &master_secret_blinding_nonce).unwrap();
    ///
    /// let mut credential_values_builder = Issuer::new_credential_values_builder().unwrap();
    /// credential_values_builder.add_value("sex", "5944657099558967239210949258394887428692050081607692519917050011144233115103").unwrap();
    /// let credential_values = credential_values_builder.finalize().unwrap();
    ///
    /// let credential_issuance_nonce = new_nonce().unwrap();
    ///
    /// let (mut credential_signature, signature_correctness_proof) =
    ///     Issuer::sign_credential("CnEDk9HrMnmiHXEV1WFgbVCRteYnPqsJwrTdcZaNhFVW",
    ///                             &blinded_master_secret,
    ///                             &blinded_master_secret_correctness_proof,
    ///                             &master_secret_blinding_nonce,
    ///                             &credential_issuance_nonce,
    ///                             &credential_values,
    ///                             &credential_pub_key,
    ///                             &credential_priv_key).unwrap();
    ///
    /// Prover::process_credential_signature(&mut credential_signature,
    ///                                      &credential_values,
    ///                                      &signature_correctness_proof,
    ///                                      &master_secret_blinding_data,
    ///                                      &master_secret,
    ///                                      &credential_pub_key,
    ///                                      &credential_issuance_nonce,
    ///                                      None, None, None).unwrap();
    ///
    /// let mut sub_proof_request_builder = Verifier::new_sub_proof_request_builder().unwrap();
    /// sub_proof_request_builder.add_revealed_attr("sex").unwrap();
    /// let sub_proof_request = sub_proof_request_builder.finalize().unwrap();
    ///
    /// let mut proof_builder = Prover::new_proof_builder().unwrap();
    /// proof_builder.add_sub_proof_request("issuer_key_id_1",
    ///                                     &sub_proof_request,
    ///                                     &credential_schema,
    ///                                     &credential_signature,
    ///                                     &credential_values,
    ///                                     &credential_pub_key,
    ///                                     None,
    ///                                     None).unwrap();
    ///
    /// let proof_request_nonce = new_nonce().unwrap();
    /// let _proof = proof_builder.finalize(&proof_request_nonce, &master_secret).unwrap();
    /// ```
    pub fn finalize(&self, nonce: &Nonce) -> Result<Proof, IndyCryptoError> {
        trace!("ProofBuilder::finalize: >>> nonce: {:?}", nonce);

        let mut values: Vec<Vec<u8>> = Vec::new();
        values.extend_from_slice(&self.tau_list);
        values.extend_from_slice(&self.c_list);
        values.push(nonce.to_bytes()?);

        // In the anoncreds whitepaper, `challenge` is denoted by `c_h`
        let challenge = get_hash_as_int(&mut values)?;

        let mut proofs: BTreeMap<String, SubProof> = BTreeMap::new();

        for (proof_cred_uuid, init_proof) in self.init_proofs.iter() {
            let mut non_revoc_proof: Option<NonRevocProof> = None;
            if let Some(ref non_revoc_init_proof) = init_proof.non_revoc_init_proof {
                non_revoc_proof = Some(ProofBuilder::_finalize_non_revocation_proof(&non_revoc_init_proof, &challenge)?);
            }

            let primary_proof = ProofBuilder::_finalize_primary_proof(&init_proof.primary_init_proof,
                                                                      &challenge,
                                                                      &init_proof.credential_schema,
                                                                      &init_proof.credential_values,
                                                                      &init_proof.sub_proof_request)?;

            let proof = SubProof { primary_proof, non_revoc_proof };
            proofs.insert(proof_cred_uuid.to_owned(), proof);
        }

        let aggregated_proof = AggregatedProof { c_hash: challenge, c_list: self.c_list.clone() };

        let proof = Proof { proofs, aggregated_proof };

        trace!("ProofBuilder::finalize: <<< proof: {:?}", proof);

        Ok(proof)
    }

    fn _check_add_sub_proof_request_params_consistency(cred_values: &CredentialValues,
                                                       sub_proof_request: &SubProofRequest,
                                                       cred_schema: &CredentialSchema) -> Result<(), IndyCryptoError> {
        trace!("ProofBuilder::_check_add_sub_proof_request_params_consistency: >>> cred_values: {:?}, sub_proof_request: {:?}, cred_schema: {:?}",
               cred_values, sub_proof_request, cred_schema);

        let cred_attrs = HashSet::from_iter(cred_values.attrs_values.keys().cloned());

        if cred_schema.attrs != cred_attrs {
            return Err(IndyCryptoError::InvalidStructure(format!("Credential doesn't correspond to credential schema")));
        }

        if sub_proof_request.revealed_attrs.difference(&cred_attrs).count() != 0 {
            return Err(IndyCryptoError::InvalidStructure(format!("Credential doesn't contain requested attribute")));
        }

        let predicates_attrs =
            sub_proof_request.predicates.iter()
                .map(|predicate| predicate.attr_name.clone())
                .collect::<HashSet<String>>();

        if predicates_attrs.difference(&cred_attrs).count() != 0 {
            return Err(IndyCryptoError::InvalidStructure(format!("Credential doesn't contain attribute requested in predicate")));
        }

        trace!("ProofBuilder::_check_add_sub_proof_request_params_consistency: <<<");

        Ok(())
    }

    fn _init_primary_proof(issuer_pub_key: &CredentialPrimaryPublicKey,
                           c1: &PrimaryCredentialSignature,
                           cred_values: &CredentialValues,
                           cred_schema: &CredentialSchema,
                           sub_proof_request: &SubProofRequest,
                           m2_t: Option<BigNumber>) -> Result<PrimaryInitProof, IndyCryptoError> {
        trace!("ProofBuilder::_init_primary_proof: >>> issuer_pub_key: {:?}, c1: {:?}, cred_values: {:?}, cred_schema: {:?}, sub_proof_request: {:?}, m2_t: {:?}",
               issuer_pub_key, c1, cred_values, cred_schema, sub_proof_request, m2_t);

        let eq_proof = ProofBuilder::_init_eq_proof(&issuer_pub_key, c1, cred_schema, sub_proof_request, m2_t)?;

        let mut ge_proofs: Vec<PrimaryPredicateGEInitProof> = Vec::new();
        for predicate in sub_proof_request.predicates.iter() {
            let ge_proof = ProofBuilder::_init_ge_proof(&issuer_pub_key, &eq_proof.m_tilde, cred_values, predicate)?;
            ge_proofs.push(ge_proof);
        }

        let primary_init_proof = PrimaryInitProof { eq_proof, ge_proofs };

        trace!("ProofBuilder::_init_primary_proof: <<< primary_init_proof: {:?}", primary_init_proof);

        Ok(primary_init_proof)
    }

    fn _init_non_revocation_proof(r_cred: &NonRevocationCredentialSignature,
                                  rev_reg: &RevocationRegistry,
                                  cred_rev_pub_key: &CredentialRevocationPublicKey,
                                  witness: &Witness) -> Result<NonRevocInitProof, IndyCryptoError> {
        trace!("ProofBuilder::_init_non_revocation_proof: >>> r_cred: {:?}, rev_reg: {:?}, cred_rev_pub_key: {:?}, witness: {:?}",
               r_cred, rev_reg, cred_rev_pub_key, witness);

        let c_list_params = ProofBuilder::_gen_c_list_params(&r_cred)?;
        let c_list = ProofBuilder::_create_c_list_values(&r_cred, &c_list_params, &cred_rev_pub_key, witness)?;

        let tau_list_params = ProofBuilder::_gen_tau_list_params()?;
        let tau_list = create_tau_list_values(&cred_rev_pub_key,
                                              &rev_reg,
                                              &tau_list_params,
                                              &c_list)?;

        let r_init_proof = NonRevocInitProof {
            c_list_params,
            tau_list_params,
            c_list,
            tau_list
        };

        trace!("ProofBuilder::_init_non_revocation_proof: <<< r_init_proof: {:?}", r_init_proof);

        Ok(r_init_proof)
    }

    fn _init_eq_proof(credr_pub_key: &CredentialPrimaryPublicKey,
                      c1: &PrimaryCredentialSignature,
                      cred_schema: &CredentialSchema,
                      sub_proof_request: &SubProofRequest,
                      m2_t: Option<BigNumber>) -> Result<PrimaryEqualInitProof, IndyCryptoError> {
        trace!("ProofBuilder::_init_eq_proof: >>> credr_pub_key: {:?}, c1: {:?}, cred_schema: {:?}, sub_proof_request: {:?}, m2_t: {:?}",
               credr_pub_key, c1, cred_schema, sub_proof_request, m2_t);

        let mut ctx = BigNumber::new_context()?;

        let m2_tilde = m2_t.unwrap_or(bn_rand(LARGE_MVECT)?);

        let r = bn_rand(LARGE_VPRIME)?;
        let e_tilde = bn_rand(LARGE_ETILDE)?;
        let v_tilde = bn_rand(LARGE_VTILDE)?;

        let unrevealed_attrs: HashSet<String> =
            cred_schema.attrs
                .difference(&sub_proof_request.revealed_attrs)
                .cloned()
                .collect::<HashSet<String>>();

        let m_tilde = get_mtilde(&unrevealed_attrs)?;

        let a_prime = credr_pub_key.s
            .mod_exp(&r, &credr_pub_key.n, Some(&mut ctx))?
            .mod_mul(&c1.a, &credr_pub_key.n, Some(&mut ctx))?;

        let v_prime = c1.v.sub(
            &c1.e.mul(&r, Some(&mut ctx))?
        )?;

        let e_prime = c1.e.sub(
            &BigNumber::from_dec("2")?.exp(&BigNumber::from_dec(&LARGE_E_START.to_string())?, Some(&mut ctx))?
        )?;

        let t = calc_teq(&credr_pub_key, &a_prime, &e_tilde, &v_tilde, &m_tilde, &m2_tilde, &unrevealed_attrs)?;

        let primary_equal_init_proof = PrimaryEqualInitProof {
            a_prime,
            t,
            e_tilde,
            e_prime,
            v_tilde,
            v_prime,
            m_tilde,
            m2_tilde: m2_tilde.clone()?,
            m2: c1.m_2.clone()?
        };

        trace!("ProofBuilder::_init_eq_proof: <<< primary_equal_init_proof: {:?}", primary_equal_init_proof);

        Ok(primary_equal_init_proof)
    }

    fn _init_ge_proof(p_pub_key: &CredentialPrimaryPublicKey,
                      m_tilde: &BTreeMap<String, BigNumber>,
                      cred_values: &CredentialValues,
                      predicate: &Predicate) -> Result<PrimaryPredicateGEInitProof, IndyCryptoError> {
        trace!("ProofBuilder::_init_ge_proof: >>> p_pub_key: {:?}, m_tilde: {:?}, cred_values: {:?}, predicate: {:?}",
               p_pub_key, m_tilde, cred_values, predicate);

        let mut ctx = BigNumber::new_context()?;
        let (k, value) = (&predicate.attr_name, predicate.value);

        let attr_value = cred_values.attrs_values.get(k.as_str())
            .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in cred_values", k)))?
            .value
            .to_dec()?
            .parse::<i32>()
            .map_err(|_| IndyCryptoError::InvalidStructure(format!("Value by key '{}' has invalid format", k)))?;

        let delta: i32 = attr_value - value;

        if delta < 0 {
            return Err(IndyCryptoError::InvalidStructure("Predicate is not satisfied".to_string()));
        }

        let u = four_squares(delta)?;

        let mut r: BTreeMap<String, BigNumber> = BTreeMap::new();
        let mut t: BTreeMap<String, BigNumber> = BTreeMap::new();
        let mut c_list: Vec<BigNumber> = Vec::new();

        for i in 0..ITERATION {
            let cur_u = u.get(&i.to_string())
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in u1", i)))?;

            let cur_r = bn_rand(LARGE_VPRIME)?;
            let cut_t = get_pedersen_commitment(&p_pub_key.z, &cur_u, &p_pub_key.s,
                                                &cur_r, &p_pub_key.n, &mut ctx)?;

            r.insert(i.to_string(), cur_r);
            t.insert(i.to_string(), cut_t.clone()?);
            c_list.push(cut_t)
        }

        let r_delta = bn_rand(LARGE_VPRIME)?;

        let t_delta = get_pedersen_commitment(&p_pub_key.z, &BigNumber::from_dec(&delta.to_string())?,
                                              &p_pub_key.s, &r_delta, &p_pub_key.n, &mut ctx)?;

        r.insert("DELTA".to_string(), r_delta);
        t.insert("DELTA".to_string(), t_delta.clone()?);
        c_list.push(t_delta);

        let mut u_tilde: BTreeMap<String, BigNumber> = BTreeMap::new();
        let mut r_tilde: BTreeMap<String, BigNumber> = BTreeMap::new();

        for i in 0..ITERATION {
            u_tilde.insert(i.to_string(), bn_rand(LARGE_UTILDE)?);
            r_tilde.insert(i.to_string(), bn_rand(LARGE_RTILDE)?);
        }

        r_tilde.insert("DELTA".to_string(), bn_rand(LARGE_RTILDE)?);
        let alpha_tilde = bn_rand(LARGE_ALPHATILDE)?;

        let mj = m_tilde.get(k.as_str())
            .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in eq_proof.mtilde", k)))?;

        let tau_list = calc_tge(&p_pub_key, &u_tilde, &r_tilde, &mj, &alpha_tilde, &t)?;

        let primary_predicate_ge_init_proof = PrimaryPredicateGEInitProof {
            c_list,
            tau_list,
            u,
            u_tilde,
            r,
            r_tilde,
            alpha_tilde,
            predicate: predicate.clone(),
            t
        };

        trace!("ProofBuilder::_init_ge_proof: <<< primary_predicate_ge_init_proof: {:?}", primary_predicate_ge_init_proof);

        Ok(primary_predicate_ge_init_proof)
    }

    fn _finalize_eq_proof(init_proof: &PrimaryEqualInitProof,
                          challenge: &BigNumber,
                          cred_schema: &CredentialSchema,
                          cred_values: &CredentialValues,
                          sub_proof_request: &SubProofRequest) -> Result<PrimaryEqualProof, IndyCryptoError> {
        trace!("ProofBuilder::_finalize_eq_proof: >>> init_proof: {:?}, challenge: {:?}, cred_schema: {:?}, \
        cred_values: {:?}, sub_proof_request: {:?}", init_proof, challenge, cred_schema, cred_values, sub_proof_request);

        let mut ctx = BigNumber::new_context()?;

        let e = challenge
            .mul(&init_proof.e_prime, Some(&mut ctx))?
            .add(&init_proof.e_tilde)?;

        let v = challenge
            .mul(&init_proof.v_prime, Some(&mut ctx))?
            .add(&init_proof.v_tilde)?;

        let mut m: BTreeMap<String, BigNumber> = BTreeMap::new();

        let unrevealed_attrs: HashSet<String> =
            cred_schema.attrs
                .difference(&sub_proof_request.revealed_attrs)
                .cloned()
                .collect::<HashSet<String>>();

        for k in unrevealed_attrs.iter() {
            let cur_mtilde = init_proof.m_tilde.get(k)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in init_proof.mtilde", k)))?;

            let cur_val = cred_values.attrs_values.get(k)
                .ok_or(IndyCryptoError::InvalidStructure(format!("Value by key '{}' not found in attributes_values", k)))?;

            let val = challenge
                .mul(&cur_val.value, Some(&mut ctx))?
                .add(&cur_mtilde)?;

            m.insert(k.clone(), val);
        }

        let m2 = challenge
            .mul(&init_proof.m2, Some(&mut ctx))?
            .add(&init_proof.m2_tilde)?;

        let mut revealed_attrs_with_values: BTreeMap<String, BigNumber> = BTreeMap::new();

        for attr in sub_proof_request.revealed_attrs.iter() {
            revealed_attrs_with_values.insert(
                attr.clone(),
                cred_values.attrs_values
                    .get(attr)
                    .ok_or(IndyCryptoError::InvalidStructure(format!("Encoded value not found")))?
                    .value
                    .clone()?,
            );
        }

        let primary_equal_proof = PrimaryEqualProof {
            revealed_attrs: revealed_attrs_with_values,
            a_prime: init_proof.a_prime.clone()?,
            e,
            v,
            m,
            m2
        };

        trace!("ProofBuilder::_finalize_eq_proof: <<< primary_equal_proof: {:?}", primary_equal_proof);

        Ok(primary_equal_proof)
    }

    fn _finalize_ge_proof(c_h: &BigNumber,
                          init_proof: &PrimaryPredicateGEInitProof,
                          eq_proof: &PrimaryEqualProof) -> Result<PrimaryPredicateGEProof, IndyCryptoError> {
        trace!("ProofBuilder::_finalize_ge_proof: >>> c_h: {:?}, init_proof: {:?}, eq_proof: {:?}", c_h, init_proof, eq_proof);

        let mut ctx = BigNumber::new_context()?;
        let mut u: BTreeMap<String, BigNumber> = BTreeMap::new();
        let mut r: BTreeMap<String, BigNumber> = BTreeMap::new();
        let mut urproduct = BigNumber::new()?;

        for i in 0..ITERATION {
            let cur_utilde = &init_proof.u_tilde[&i.to_string()];
            let cur_u = &init_proof.u[&i.to_string()];
            let cur_rtilde = &init_proof.r_tilde[&i.to_string()];
            let cur_r = &init_proof.r[&i.to_string()];

            let new_u: BigNumber = c_h
                .mul(&cur_u, Some(&mut ctx))?
                .add(&cur_utilde)?;
            let new_r: BigNumber = c_h
                .mul(&cur_r, Some(&mut ctx))?
                .add(&cur_rtilde)?;

            u.insert(i.to_string(), new_u);
            r.insert(i.to_string(), new_r);

            urproduct = cur_u
                .mul(&cur_r, Some(&mut ctx))?
                .add(&urproduct)?;

            let cur_rtilde_delta = &init_proof.r_tilde["DELTA"];

            let new_delta = c_h
                .mul(&init_proof.r["DELTA"], Some(&mut ctx))?
                .add(&cur_rtilde_delta)?;

            r.insert("DELTA".to_string(), new_delta);
        }

        let alpha = init_proof.r["DELTA"]
            .sub(&urproduct)?
            .mul(&c_h, Some(&mut ctx))?
            .add(&init_proof.alpha_tilde)?;

        let primary_predicate_ge_proof = PrimaryPredicateGEProof {
            u,
            r,
            mj: eq_proof.m[&init_proof.predicate.attr_name].clone()?,
            alpha,
            t: clone_bignum_map(&init_proof.t)?,
            predicate: init_proof.predicate.clone()
        };

        trace!("ProofBuilder::_finalize_ge_proof: <<< primary_predicate_ge_proof: {:?}", primary_predicate_ge_proof);

        Ok(primary_predicate_ge_proof)
    }

    fn _finalize_primary_proof(init_proof: &PrimaryInitProof,
                               challenge: &BigNumber,
                               cred_schema: &CredentialSchema,
                               cred_values: &CredentialValues,
                               sub_proof_request: &SubProofRequest) -> Result<PrimaryProof, IndyCryptoError> {
        trace!("ProofBuilder::_finalize_primary_proof: >>> init_proof: {:?}, challenge: {:?}, cred_schema: {:?}, \
        cred_values: {:?}, sub_proof_request: {:?}", init_proof, challenge, cred_schema, cred_values, sub_proof_request);

        let eq_proof = ProofBuilder::_finalize_eq_proof(&init_proof.eq_proof, challenge, cred_schema, cred_values, sub_proof_request)?;
        let mut ge_proofs: Vec<PrimaryPredicateGEProof> = Vec::new();

        for init_ge_proof in init_proof.ge_proofs.iter() {
            let ge_proof = ProofBuilder::_finalize_ge_proof(challenge, init_ge_proof, &eq_proof)?;
            ge_proofs.push(ge_proof);
        }

        let primary_proof = PrimaryProof { eq_proof, ge_proofs };

        trace!("ProofBuilder::_finalize_primary_proof: <<< primary_proof: {:?}", primary_proof);

        Ok(primary_proof)
    }

    fn _gen_c_list_params(r_cred: &NonRevocationCredentialSignature) -> Result<NonRevocProofXList, IndyCryptoError> {
        trace!("ProofBuilder::_gen_c_list_params: >>> r_cred: {:?}", r_cred);

        let rho = GroupOrderElement::new()?;
        let r = GroupOrderElement::new()?;
        let r_prime = GroupOrderElement::new()?;
        let r_prime_prime = GroupOrderElement::new()?;
        let r_prime_prime_prime = GroupOrderElement::new()?;
        let o = GroupOrderElement::new()?;
        let o_prime = GroupOrderElement::new()?;
        let m = rho.mul_mod(&r_cred.c)?;
        let m_prime = r.mul_mod(&r_prime_prime)?;
        let t = o.mul_mod(&r_cred.c)?;
        let t_prime = o_prime.mul_mod(&r_prime_prime)?;
        let m2 = GroupOrderElement::from_bytes(&r_cred.m2.to_bytes()?)?;

        let non_revoc_proof_x_list = NonRevocProofXList {
            rho,
            r,
            r_prime,
            r_prime_prime,
            r_prime_prime_prime,
            o,
            o_prime,
            m,
            m_prime,
            t,
            t_prime,
            m2,
            s: r_cred.vr_prime_prime,
            c: r_cred.c
        };

        trace!("ProofBuilder::_gen_c_list_params: <<< non_revoc_proof_x_list: {:?}", non_revoc_proof_x_list);

        Ok(non_revoc_proof_x_list)
    }

    fn _create_c_list_values(r_cred: &NonRevocationCredentialSignature,
                             params: &NonRevocProofXList,
                             r_pub_key: &CredentialRevocationPublicKey,
                             witness: &Witness) -> Result<NonRevocProofCList, IndyCryptoError> {
        trace!("ProofBuilder::_create_c_list_values: >>> r_cred: {:?}, r_pub_key: {:?}", r_cred, r_pub_key);

        let e = r_pub_key.h
            .mul(&params.rho)?
            .add(
                &r_pub_key.htilde.mul(&params.o)?
            )?;

        let d = r_pub_key.g
            .mul(&params.r)?
            .add(
                &r_pub_key.htilde.mul(&params.o_prime)?
            )?;

        let a = r_cred.sigma
            .add(
                &r_pub_key.htilde.mul(&params.rho)?
            )?;

        let g = r_cred.g_i
            .add(
                &r_pub_key.htilde.mul(&params.r)?
            )?;

        let w = witness.omega
            .add(
                &r_pub_key.h_cap.mul(&params.r_prime)?
            )?;

        let s = r_cred.witness_signature.sigma_i
            .add(
                &r_pub_key.h_cap.mul(&params.r_prime_prime)?
            )?;

        let u = r_cred.witness_signature.u_i
            .add(
                &r_pub_key.h_cap.mul(&params.r_prime_prime_prime)?
            )?;

        let non_revoc_proof_c_list = NonRevocProofCList {
            e,
            d,
            a,
            g,
            w,
            s,
            u
        };

        trace!("ProofBuilder::_create_c_list_values: <<< non_revoc_proof_c_list: {:?}", non_revoc_proof_c_list);

        Ok(non_revoc_proof_c_list)
    }

    fn _gen_tau_list_params() -> Result<NonRevocProofXList, IndyCryptoError> {
        trace!("ProofBuilder::_gen_tau_list_params: >>>");

        let non_revoc_proof_x_list = NonRevocProofXList {
            rho: GroupOrderElement::new()?,
            r: GroupOrderElement::new()?,
            r_prime: GroupOrderElement::new()?,
            r_prime_prime: GroupOrderElement::new()?,
            r_prime_prime_prime: GroupOrderElement::new()?,
            o: GroupOrderElement::new()?,
            o_prime: GroupOrderElement::new()?,
            m: GroupOrderElement::new()?,
            m_prime: GroupOrderElement::new()?,
            t: GroupOrderElement::new()?,
            t_prime: GroupOrderElement::new()?,
            m2: GroupOrderElement::new()?,
            s: GroupOrderElement::new()?,
            c: GroupOrderElement::new()?
        };

        trace!("ProofBuilder::_gen_tau_list_params: <<< Nnon_revoc_proof_x_list: {:?}", non_revoc_proof_x_list);

        Ok(non_revoc_proof_x_list)
    }

    fn _finalize_non_revocation_proof(init_proof: &NonRevocInitProof, c_h: &BigNumber) -> Result<NonRevocProof, IndyCryptoError> {
        trace!("ProofBuilder::_finalize_non_revocation_proof: >>> init_proof: {:?}, c_h: {:?}", init_proof, c_h);

        let ch_num_z = bignum_to_group_element(&c_h)?;
        let mut x_list: Vec<GroupOrderElement> = Vec::new();

        for (x, y) in init_proof.tau_list_params.as_list()?.iter().zip(init_proof.c_list_params.as_list()?.iter()) {
            x_list.push(x.add_mod(
                &ch_num_z.mul_mod(&y)?.mod_neg()?
            )?);
        }

        let non_revoc_proof = NonRevocProof {
            x_list: NonRevocProofXList::from_list(x_list),
            c_list: init_proof.c_list.clone()
        };

        trace!("ProofBuilder::_finalize_non_revocation_proof: <<< non_revoc_proof: {:?}", non_revoc_proof);

        Ok(non_revoc_proof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cl::issuer;
    use pair::PairMocksHelper;

    #[ignore]
    #[test]
    fn generate_master_secret_works() {
        MockHelper::inject();

        let ms = Prover::new_master_secret().unwrap();
        assert_eq!(ms.ms.to_dec().unwrap(), mocks::master_secret().ms.to_dec().unwrap());
    }

    #[test]
    fn generate_primary_blinded_credential_secrets_works() {
        MockHelper::inject();

        let primary_blinded_credential_secrets =
            Prover::_generate_primary_blinded_credential_secrets(&issuer::mocks::credential_primary_public_key(),
                                                                 &mocks::credential_values()).unwrap();
        assert_eq!(primary_blinded_credential_secrets, mocks::primary_blinded_credential_secrets_factors());
    }

    #[test]
    fn generate_revocation_blinded_credential_secrets_works() {
        MockHelper::inject();

        let r_pk = issuer::mocks::credential_revocation_public_key();
        Prover::_generate_revocation_blinded_credential_secrets(&r_pk).unwrap();
    }

    #[test]
    fn generate_blinded_credential_secrets_works() {
        MockHelper::inject();
        PairMocksHelper::inject();

        let (blinded_credential_secrets,
             credential_secrets_blinding_factors,
             blinded_credential_secrets_correctness_proof) =
                Prover::blind_credential_secrets(&issuer::mocks::credential_public_key(),
                                                 &issuer::mocks::credential_key_correctness_proof(),
                                                 &mocks::credential_values(),
                                                 &mocks::credential_nonce()).unwrap();

        assert_eq!(blinded_credential_secrets.u, mocks::primary_blinded_credential_secrets_factors().u);
        assert_eq!(credential_secrets_blinding_factors.v_prime, mocks::primary_blinded_credential_secrets_factors().v_prime);
        assert_eq!(blinded_credential_secrets.committed_attributes, mocks::primary_blinded_credential_secrets_factors().committed_attributes);
        assert!(blinded_credential_secrets.ur.is_some());
        assert!(credential_secrets_blinding_factors.vr_prime.is_some());
        assert_eq!(blinded_credential_secrets_correctness_proof, mocks::blinded_credential_secrets_correctness_proof())
    }

    #[test]
    fn process_primary_credential_works() {
        MockHelper::inject();

        let mut credential = issuer::mocks::primary_credential();
        let v_prime = mocks::primary_blinded_credential_secrets_factors().v_prime;

        Prover::_process_primary_credential(&mut credential, &v_prime).unwrap();

        assert_eq!(mocks::primary_credential(), credential);
    }

    #[ignore]
    #[test]
    fn process_credential_signature_works() {
        MockHelper::inject();

        let mut credential_signature = issuer::mocks::credential();

        Prover::process_credential_signature(&mut credential_signature,
                                             &mocks::credential_values(),
                                             &issuer::mocks::signature_correctness_proof(),
                                             &mocks::credential_secrets_blinding_factors(),
                                             &issuer::mocks::credential_public_key(),
                                             &issuer::mocks::credential_issuance_nonce(),
                                             None,
                                             None,
                                             None).unwrap();

        assert_eq!(mocks::primary_credential(), credential_signature.p_credential);
    }

//    #[test]
//    fn init_eq_proof_works() {
//        MockHelper::inject();
//
//        let pk = issuer::mocks::credential_primary_public_key();
//        let credential_schema = issuer::mocks::credential_schema();
//        let credential = mocks::primary_credential();
//        let sub_proof_request = mocks::sub_proof_request();
//        let m1_t = mocks::m1_t();
//
//        let init_eq_proof = ProofBuilder::_init_eq_proof(&pk,
//                                                         &credential,
//                                                         &credential_schema,
//                                                         &sub_proof_request,
//                                                         &m1_t,
//                                                         None).unwrap();
//
//        assert_eq!(mocks::primary_equal_init_proof(), init_eq_proof);
//    }
//
//    #[test]
//    fn init_ge_proof_works() {
//        MockHelper::inject();
//
//        let pk = issuer::mocks::credential_primary_public_key();
//        let init_eq_proof = mocks::primary_equal_init_proof();
//        let predicate = mocks::predicate();
//        let credential_schema = issuer::mocks::credential_values();
//
//        let init_ge_proof = ProofBuilder::_init_ge_proof(&pk,
//                                                         &init_eq_proof.m_tilde,
//                                                         &credential_schema,
//                                                         &predicate).unwrap();
//
//        assert_eq!(mocks::primary_ge_init_proof(), init_ge_proof);
//    }
//
//    #[test]
//    fn init_primary_proof_works() {
//        MockHelper::inject();
//
//        let pk = issuer::mocks::credential_primary_public_key();
//        let credential_schema = issuer::mocks::credential_schema();
//        let credential = mocks::credential();
//        let m1_t = mocks::m1_t();
//        let credential_values = issuer::mocks::credential_values();
//        let sub_proof_request = mocks::sub_proof_request();
//
//        let init_proof = ProofBuilder::_init_primary_proof(&pk,
//                                                           &credential.p_credential,
//                                                           &credential_values,
//                                                           &credential_schema,
//                                                           &sub_proof_request,
//                                                           &m1_t,
//                                                           None).unwrap();
//        assert_eq!(mocks::primary_init_proof(), init_proof);
//    }
//
//    #[test]
//    fn finalize_eq_proof_works() {
//        MockHelper::inject();
//
//        let ms = mocks::master_secret();
//        let c_h = mocks::aggregated_proof().c_hash;
//        let init_proof = mocks::primary_equal_init_proof();
//        let credential_values = issuer::mocks::credential_values();
//        let credential_schema = issuer::mocks::credential_schema();
//        let sub_proof_request = mocks::sub_proof_request();
//
//        let eq_proof = ProofBuilder::_finalize_eq_proof(&ms.ms,
//                                                        &init_proof,
//                                                        &c_h,
//                                                        &credential_schema,
//                                                        &credential_values,
//                                                        &sub_proof_request).unwrap();
//
//        assert_eq!(mocks::eq_proof(), eq_proof);
//    }
//
//    #[test]
//    fn finalize_ge_proof_works() {
//        MockHelper::inject();
//
//        let c_h = mocks::aggregated_proof().c_hash;
//        let ge_proof = mocks::primary_ge_init_proof();
//        let eq_proof = mocks::eq_proof();
//
//        let ge_proof = ProofBuilder::_finalize_ge_proof(&c_h,
//                                                        &ge_proof,
//                                                        &eq_proof).unwrap();
//        assert_eq!(mocks::ge_proof(), ge_proof);
//    }
//
//    #[test]
//    fn finalize_primary_proof_works() {
//        MockHelper::inject();
//
//        let proof = mocks::primary_init_proof();
//        let ms = mocks::master_secret();
//        let c_h = mocks::aggregated_proof().c_hash;
//        let credential_schema = issuer::mocks::credential_schema();
//        let credential_values = issuer::mocks::credential_values();
//        let sub_proof_request = mocks::sub_proof_request();
//
//        let proof = ProofBuilder::_finalize_primary_proof(&ms.ms,
//                                                          &proof,
//                                                          &c_h,
//                                                          &credential_schema,
//                                                          &credential_values,
//                                                          &sub_proof_request).unwrap();
//
//        assert_eq!(mocks::primary_proof(), proof);
//    }
//
//    #[test]
//    fn test_witness_credential_works() {
//        let mut r_credential = issuer::mocks::revocation_credential();
//        let r_key = issuer::mocks::credential_revocation_public_key();
//        let rev_key_pub = issuer::mocks::revocation_key_public();
//        let rev_reg = issuer::mocks::revocation_registry();
//        let witness = issuer::mocks::witness();
//        let r_cnxt_m2 = issuer::mocks::r_cnxt_m2();
//
//        Prover::_test_witness_signature(&mut r_credential, &r_key, &rev_key_pub, &rev_reg, &witness, &r_cnxt_m2).unwrap();
//    }
//
//    #[test]
//    fn test_c_and_tau_list() {
//        let r_credential = issuer::mocks::revocation_credential();
//        let r_key = issuer::mocks::credential_revocation_public_key();
//        let rev_pub_key = issuer::mocks::revocation_key_public();
//        let rev_reg = issuer::mocks::revocation_registry();
//        let witness = issuer::mocks::witness();
//
//        let c_list_params = ProofBuilder::_gen_c_list_params(&r_credential).unwrap();
//
//        let proof_c_list = ProofBuilder::_create_c_list_values(&r_credential, &c_list_params, &r_key, &witness).unwrap();
//
//        let proof_tau_list = create_tau_list_values(&r_key, &rev_reg,
//                                                    &c_list_params, &proof_c_list).unwrap();
//
//        let proof_tau_list_calc = create_tau_list_expected_values(&r_key,
//                                                                  &rev_reg,
//                                                                  &rev_pub_key,
//                                                                  &proof_c_list).unwrap();
//
//        assert_eq!(proof_tau_list.as_slice().unwrap(), proof_tau_list_calc.as_slice().unwrap());
//    }
//
//    extern crate time;
//
//    /*
//    Results:
//
//    N = 100
//    Create RevocationRegistry Time: Duration { secs: 0, nanos: 153759082 }
//    Update NonRevocation Credential Time: Duration { secs: 0, nanos: 490382 }
//    Total Time for 100 credentials: Duration { secs: 5, nanos: 45915383 }
//
//    N = 1000
//    Create RevocationRegistry Time: Duration { secs: 1, nanos: 636113212 }
//    Update NonRevocation Credential Time: Duration { secs: 0, nanos: 5386575 }
//    Total Time for 1000 credentials: Duration { secs: 6, nanos: 685771457 }
//
//    N = 10000
//    Create RevocationRegistry Time: Duration { secs: 16, nanos: 844061103 }
//    Update NonRevocation Credential Time: Duration { secs: 0, nanos: 52396763 }
//    Total Time for 10000 credentials: Duration { secs: 29, nanos: 628240611 }
//
//    N = 100000
//    Create RevocationRegistry Time: Duration { secs: 175, nanos: 666428558 }
//    Update NonRevocation Credential Time: Duration { secs: 0, nanos: 667879620 }
//    Total Time for 100000 credentials: Duration { secs: 185, nanos: 810126906 }
//
//    N = 1000000
//    Create RevocationRegistry Time: Duration { secs: 1776, nanos: 485208599 }
//    Update NonRevocation Credential Time: Duration { secs: 6, nanos: 35027554 }
//    Total Time for 1000000 credentials: Duration { secs: 1798, nanos: 420564334 }
//    */
//    #[test]
//    fn test_update_proof() {
//        println!("Update Proof test -> start");
//        let n = 100;
//
//        let total_start_time = time::get_time();
//
//        let cred_schema = issuer::mocks::credential_schema();
//        let (cred_pub_key, cred_priv_key, cred_key_correctness_proof) = issuer::Issuer::new_credential_def(&cred_schema, true).unwrap();
//
//        let start_time = time::get_time();
//
//        let (rev_key_pub, rev_key_priv, mut rev_reg, mut rev_tails_generator) = issuer::Issuer::new_revocation_registry_def(&cred_pub_key, n, false).unwrap();
//
//        let simple_tail_accessor = SimpleTailsAccessor::new(&mut rev_tails_generator).unwrap();
//
//        let end_time = time::get_time();
//
//        println!("Create RevocationRegistry Time: {:?}", end_time - start_time);
//
//        let cred_values = issuer::mocks::credential_values();
//
//        // Issue first correct Claim
//        let master_secret = Prover::new_master_secret().unwrap();
//        let master_secret_blinding_nonce = new_nonce().unwrap();
//
//        let (blinded_master_secret, master_secret_blinding_data, blinded_master_secret_correctness_proof) =
//            Prover::blind_credential_secrets(&cred_pub_key,
//                                             &cred_key_correctness_proof,
//                                             &master_secret,
//                                             &master_secret_blinding_nonce).unwrap();
//
//        let cred_issuance_nonce = new_nonce().unwrap();
//
//        let rev_idx = 1;
//        let (mut cred_signature, signature_correctness_proof, rev_reg_delta) =
//            issuer::Issuer::sign_credential_with_revoc("CnEDk9HrMnmiHXEV1WFgbVCRteYnPqsJwrTdcZaNhFVW",
//                                                       &blinded_master_secret,
//                                                       &blinded_master_secret_correctness_proof,
//                                                       &master_secret_blinding_nonce,
//                                                       &cred_issuance_nonce,
//                                                       &cred_values,
//                                                       &cred_pub_key,
//                                                       &cred_priv_key,
//                                                       rev_idx,
//                                                       n,
//                                                       false,
//                                                       &mut rev_reg,
//                                                       &rev_key_priv,
//                                                       &simple_tail_accessor).unwrap();
//        let mut rev_reg_delta = rev_reg_delta.unwrap();
//
//        let mut witness = Witness::new(rev_idx, n, &rev_reg_delta, &simple_tail_accessor).unwrap();
//
//        Prover::process_credential_signature(&mut cred_signature,
//                                             &cred_values,
//                                             &signature_correctness_proof,
//                                             &master_secret_blinding_data,
//                                             &master_secret,
//                                             &cred_pub_key,
//                                             &cred_issuance_nonce,
//                                             Some(&rev_key_pub),
//                                             Some(&rev_reg),
//                                             Some(&witness)).unwrap();
//
//        // Populate accumulator
//        for i in 2..n {
//            let index = n + 1 - i;
//
//            simple_tail_accessor.access_tail(index, &mut |tail| {
//                rev_reg_delta.accum = rev_reg_delta.accum.sub(tail).unwrap();
//            }).unwrap();
//
//            rev_reg_delta.issued.insert(i);
//        }
//
//        // Update NonRevoc Credential
//
//        let start_time = time::get_time();
//
//        witness.update(rev_idx, n, &rev_reg_delta, &simple_tail_accessor).unwrap();
//
//        let end_time = time::get_time();
//
//        println!("Update NonRevocation Credential Time: {:?}", end_time - start_time);
//
//        let total_end_time = time::get_time();
//        println!("Total Time for {} credentials: {:?}", n, total_end_time - total_start_time);
//
//        println!("Update Proof test -> end");
//    }
}

pub mod mocks {
    use std::iter::FromIterator;
    use super::*;

    pub const PROVER_DID: &'static str = "CnEDk9HrMnmiHXEV1WFgbVCRteYnPqsJwrTdcZaNhFVW";

    pub fn master_secret() -> MasterSecret {
        MasterSecret {
            ms: link_secret()
        }
    }

    pub fn link_secret() -> BigNumber {
        BigNumber::from_dec("34940487469005237202297983870352092482682591325620866393958633274031736589516").unwrap()
    }

    pub fn link_secret_blinding_factor() -> BigNumber {
        BigNumber::from_dec("12403977281408319830341147313026597120312785716035135330072781873547187561426").unwrap()
    }

    pub fn policy_address() -> BigNumber {
        BigNumber::from_dec("82482513509927463198200988655461469819592280137503867166383914706498311851913").unwrap()
    }

    pub fn policy_address_blinding_factor() -> BigNumber {
        BigNumber::from_dec("101896356200142281702846875799022863451783539174051329030463640228462536469916").unwrap()
    }

    pub fn credential_nonce() -> Nonce {
        BigNumber::from_dec("526193306511429638192053").unwrap()
    }

    pub fn credential_schema() -> CredentialSchema {
        CredentialSchema {
            attrs: hashset![
                String::from("link_secret"),
                String::from("policy_address"),
                String::from("name"),
                String::from("gender"),
                String::from("age"),
                String::from("height")
            ]
        }
    }

    pub fn credential_values() -> CredentialValues {
        CredentialValues {
            attrs_values: btreemap![
                String::from("link_secret") => CredentialValue { value: link_secret(), blinding_factor: Some(link_secret_blinding_factor()) },
                String::from("policy_address") => CredentialValue { value: policy_address(), blinding_factor: Some(policy_address_blinding_factor()) },
                String::from("name") => CredentialValue { value: BigNumber::from_dec("71359565546479723151967460283929432570283558415909434050407244812473401631735").unwrap(), blinding_factor: None },
                String::from("gender") => CredentialValue { value: BigNumber::from_dec("1796449712852417654363673724889734415544693752249017564928908250031932273569").unwrap(), blinding_factor: None },
                String::from("age") => CredentialValue { value: BigNumber::from_dec("35").unwrap(), blinding_factor: None },
                String::from("height") => CredentialValue { value: BigNumber::from_dec("175").unwrap(), blinding_factor: None }
            ]
        }
    }

    pub fn blinded_credential_secrets() -> BlindedCredentialSecrets {
        BlindedCredentialSecrets {
            u: primary_blinded_credential_secrets_factors().u,
            ur: Some(revocation_blinded_credential_secrets_factors().ur),
            committed_attributes: primary_blinded_credential_secrets_factors().committed_attributes
        }
    }

    pub fn credential_secrets_blinding_factors() -> CredentialSecretsBlindingFactors {
        CredentialSecretsBlindingFactors {
            v_prime: primary_blinded_credential_secrets_factors().v_prime,
            vr_prime: Some(revocation_blinded_credential_secrets_factors().vr_prime)
        }
    }

    pub fn primary_blinded_credential_secrets_factors() -> PrimaryBlindedCredentialSecretsFactors {
        PrimaryBlindedCredentialSecretsFactors {
            u: BigNumber::from_dec("47723789467324780675596875081347747479320627810048281093901400047762979563059906556791220135858818030513899970550825333284342841510800678843474627885555246105908411614394363087122961850889010634506499101410088942045336606938075323428990497144271795963654705507552770440603826268000601604722427749968097516106288723402025571295850992636738524478503550849567356041275809736561947892778594556789352642394950858071131551896302034046337284082758795918249422986376466412526903587523169139938125179844417875931185766113421874861290614367429698444482776956343469971398441588630248177425682784201204788643072267753274494264901").unwrap(),
            v_prime: BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap(),
            committed_attributes: btreemap![
                String::from("link_secret") => BigNumber::from_dec("49969020507190580496384980443375026645795643078723554989432722612271316286345977131200174869277971717007592597917419990181140557081708512015680125983736005219218528519439235076073937590173905623762709849007022671201338968361615196582852079568408731126336593621645992928388757193176105581015724206714348547852560310565668922932721254044991756946832643303642630843853562068790572869218204826766322905248567100262495545026829880492066755881269864160215522745706175041832277294718289741382425008199559837378764356039293153774252452817588786947643439537318086516103762457197643649364432829542165643202545479218389206706033").unwrap(),
            	String::from("policy_address") => BigNumber::from_dec("16512991906262704046800797493113836808625024056440541878982888887135377828874513959220693007368983840983821076434876234772902313463147534099460345183575535990533514172142015769098079829268367243455106505017771112504793053822345286075231480355678986100675758337947488471082827474920586939076394805894487683684819392687544307860172436135923585873807385179022376950945069482169498353678436947132938825962017101586551876133429459351856183810153988844462698151736644008416755826793154986810121384261183814355962607395564486323344191001322903290092140473231352138036599231791358927316124031959887532959822363299993578986167").unwrap()
            ]
        }
    }

    pub fn revocation_blinded_credential_secrets_factors() -> RevocationBlindedCredentialSecretsFactors {
        RevocationBlindedCredentialSecretsFactors {
            ur: PointG1::from_string("false CFFE6ECFE88B20 D07CD714AF7D2D 2A5B4CBEA3C20 9F01A39E9CAC4 D65FB18 853E49F76DED9D 8FD8E08920FA65 D60F7F43C2ED2 A5800960965DF0 EB86FB4 FFFFFF7D07A8A8 FFFF7888802F07 FFC63D474548B7 F417D05FB10933 95E45DD").unwrap(),
            vr_prime: GroupOrderElement::from_string("B7D7DC1499EA50 6F16C9B5FE2C00 466542B923D8C9 FB01F2122DE924 22EB5716").unwrap()
        }
    }

    pub fn blinded_credential_secrets_correctness_proof() -> BlindedCredentialSecretsCorrectnessProof {
        BlindedCredentialSecretsCorrectnessProof {
            c: BigNumber::from_dec("14841172341445426371917159789882861318659491120199315124950674390948981175338").unwrap(),
            v_dash_cap: BigNumber::from_dec("28516187632169681034916317443066293542232820810520653219879290887490340188705207618485009504398168546052096659258653141472769442696152483795243203469420209533179847395689362264834456775325717804994642923912063868921099846607636317863802802672778955777043168774014041446601376892833320977158611770988292609505614913051087602864499964960751982426322040423136416286284406778723768615346312560758665956734914956608857741965214512689579120444173256056732313480878920238915447726647878160372621420916500391951553276433704739590206059846111800044504657850998207290228383614442391753642755750078961882529308856014893150296449437166082320118361075611513317395770555550769579256355761018140448633803961757818524074080896040113").unwrap(),
            m_caps: btreemap![
                String::from("link_secret") => BigNumber::from_dec("10838856720335086997514320436220050141007701230661779081104856968960237995899875420397944567372033031325529436064663183433074406626856806034411450616724698556099954684967863354423").unwrap(),
                String::from("policy_address") => BigNumber::from_dec("10838856720335086997514321141799452075820847986207752249555482812708631912670851435083274552034024251776525051133958921295960051019088305245368039883708358320016732424108678519609").unwrap()
            ],
            r_caps: btreemap![
                String::from("link_secret") => BigNumber::from_dec("10838856720335086997514320101751818472141253904578162256541111130344841097563151430982692259046551329904608720560865502180754767027256832200288906132220086190905660199234086110003").unwrap(),
                String::from("policy_address") => BigNumber::from_dec("10838856720335086997514321429923637251009481240166132223510744434756474932726905593474781164587782282964820431454776584307478690721232031349082655900710105805047187974429692929623").unwrap()
            ]
        }
    }

    pub fn credential() -> CredentialSignature {
        CredentialSignature {
            p_credential: primary_credential(),
            r_credential: Some(issuer::mocks::revocation_credential())
        }
    }

//    pub fn m1_t() -> BigNumber {
//        BigNumber::from_dec("67940925789970108743024738273926421512152745397724199848594503731042154269417576665420030681245389493783225644817826683796657351721363490290016166310023506339911751676800452438014771736117676826911321621579680668201191205819012441197794443970687648330757835198888257781967404396196813475280544039772512800509").unwrap()
//    }
//
    pub fn primary_credential() -> PrimaryCredentialSignature {
        PrimaryCredentialSignature {
            m_2: BigNumber::from_dec("69277050336954731912953999596899794023422356864020449587821228635678593076726").unwrap(),
            a: BigNumber::from_dec("59576650729757014001266415495048651014371875592452607038291814388111315911848291102110497520252073850059895120162321101149178450199060886721905586280704576277966012808672880874635221160361161880289965721881196877768108586929304787520934823952926459697296988463659936307341225404107666757142995151042428995859916215979973310048071060845782364280443800907503315348117229725994495933944492229338552866398085646193855519664376641572721062419860464686594200135802645323748450574040748736978128316478992822369888152234869857797942929992424339164545866301175765533370751998934550269667261831972378348502681712036651084791677").unwrap(),
            e: BigNumber::from_dec("259344723055062059907025491480697571938277889515152306249728583105665800713306759149981690559193987143012367913206299323899696942213235956742930201588264091397308910346117473868881").unwrap(),
            v: BigNumber::from_dec("6620937836014079781509458870800001917950459774302786434315639456568768602266735503527631640833663968617512880802104566048179854406925811731340920442625764155409951969854303612644127544973467090784833169581477025096651956458587024481106269073426545688878633368395090950721246745797130514914475184220252785922714892764536041334549342283500382915967329086709002330282037812607548379718641877595592743676836398647524633348205332354808351273389207425490367080293557186321576642355686995967422099839906367044852871358174711678743078106239862383119503287568833606375474359241383490799700740580296717320354647238288294827855343155547056851646090370313395520915221874011198982966904484363631910557996205942678772502957389321620232931357572315089162587705606682143499451357592399858038685832965830759409094928957246320485487746463").unwrap()
        }
    }

//    pub fn primary_init_proof() -> PrimaryInitProof {
//        PrimaryInitProof {
//            eq_proof: primary_equal_init_proof(),
//            ge_proofs: vec![primary_ge_init_proof()]
//        }
//    }
//
//    pub fn primary_equal_init_proof() -> PrimaryEqualInitProof {
//        let a_prime = BigNumber::from_dec("71198415862588101794999647637020594298636904952221229203758282286975648719760139091058820193148109269247332893072500009542535008873854752148253162724944592022459474653064164142982594342926411034455992098661321743462319688749656666526142178124484745737199241840970729963874025117751516490879240004090076615289806927701165887254974076649588902577976777511906325622743656262704616698456422853985442045734201762141883277985205745253481177231940188177322557579410753761153630309562334285168209207788901648739373257862961666829476892899815574748297248950737715666360295849203006237827045519446375662564835999315073290305487").unwrap();
//        let t = BigNumber::from_dec("37079530399722470518553835765280909308924406195904537678706963737490514431969110883727762237489123135983690261368793099989547448050747260585120834115084482614671513768476918647769960328169587408048655264846527352389174831143008287892168564249124614290079578751533814072028575120651597983808151398302966613224948742301708922129750198808877460802873030542484563816871765427025336010548850910648439965691024868634556032480548923062720951911497199235783825753860665261995069217380645765606332915666031431712257374872377210061771504712087791540252026824755757495835057557581532161492715832664496193177508138480821897473809").unwrap();
//        let e_tilde = BigNumber::from_dec("162083298053730499878539835193560156486733663622707027216327685550780519347628838870322946818623352681120371349972731968874009673965057322").unwrap();
//        let e_prime = BigNumber::from_dec("524456141360955985047633523128638545").unwrap();
//        let v_tilde = BigNumber::from_dec("241132863422049783305938184561371219250127488499746090592218003869595412171810997360214885239402274273939963489505434726467041932541499422544431299362364797699330176612923593931231233163363211565697860685967381420219969754969010598350387336530924879073366177641099382257720898488467175132844984811431059686249020737675861448309521855120928434488546976081485578773933300425198911646071284164884533755653094354378714645351464093907890440922615599556866061098147921890790915215227463991346847803620736586839786386846961213073783437136210912924729098636427160258710930323242639624389905049896225019051952864864612421360643655700799102439682797806477476049234033513929028472955119936073490401848509891547105031112859155855833089675654686301183778056755431562224990888545742379494795601542482680006851305864539769704029428620446639445284011289708313620219638324467338840766574612783533920114892847440641473989502440960354573501").unwrap();
//        let v_prime = BigNumber::from_dec("6122626610060688577826028713229499074477199356382901788064599481139201841946675307459429492073681684106974266732473283582251199684473394004038677069391278799297504466809439456560373351261561843732294201399342642485048861806520699838955215375938183164246905713902888830173868746004110336429406019431751890876414837974585857037931936009631605481447289893116786856562441832216311257042439806063785598342878372454731622929805073343996197573787090352073902245810345895873431467898909436762044613966967021911486188119609549831292025135993050932365492572744590585266402690739158346280929929978500499339008113747791946209747828024836255098012541106593813811665807502701513851726770557955311255012143102074491761548144980609065262303926782928259410970230923851333959833714917949253189276799418924788811164548907247060119625232347").unwrap();
//        let m_tilde = mocks::mtilde();
//
//        let m2_tilde = BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap();
//        let m2 = BigNumber::from_dec("79198861930494722247098854124679815411215565468368019592091735771996515839812").unwrap();
//
//        PrimaryEqualInitProof {
//            a_prime,
//            t,
//            e_tilde,
//            e_prime,
//            v_tilde,
//            v_prime,
//            m_tilde,
//            m2_tilde,
//            m2
//        }
//    }
//
//    pub fn primary_ge_init_proof() -> PrimaryPredicateGEInitProof {
//        let c_list: Vec<BigNumber> = c_list();
//        let tau_list: Vec<BigNumber> = tau_list();
//
//        let mut u: HashMap<String, BigNumber> = HashMap::new();
//        u.insert("0".to_string(), BigNumber::from_dec("3").unwrap());
//        u.insert("1".to_string(), BigNumber::from_dec("1").unwrap());
//        u.insert("2".to_string(), BigNumber::from_dec("0").unwrap());
//        u.insert("3".to_string(), BigNumber::from_dec("0").unwrap());
//
//        let mut u_tilde = HashMap::new();
//        u_tilde.insert("3".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        u_tilde.insert("1".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        u_tilde.insert("2".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        u_tilde.insert("0".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//
//        let mut r = HashMap::new();
//        r.insert("3".to_string(), BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap());
//        r.insert("1".to_string(), BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap());
//        r.insert("2".to_string(), BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap());
//        r.insert("0".to_string(), BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap());
//        r.insert("DELTA".to_string(), BigNumber::from_dec("1921424195886158938744777125021406748763985122590553448255822306242766229793715475428833504725487921105078008192433858897449555181018215580757557939320974389877538474522876366787859030586130885280724299566241892352485632499791646228580480458657305087762181033556428779333220803819945703716249441372790689501824842594015722727389764537806761583087605402039968357991056253519683582539703803574767702877615632257021995763302779502949501243649740921598491994352181379637769188829653918416991301420900374928589100515793950374255826572066003334385555085983157359122061582085202490537551988700484875690854200826784921400257387622318582276996322436").unwrap());
//
//        let mut r_tilde = HashMap::new();
//        r_tilde.insert("3".to_string(), BigNumber::from_dec("7575191721496255329790454166600075461811327744716122725414003704363002865687003988444075479817517968742651133011723131465916075452356777073568785406106174349810313776328792235352103470770562831584011847").unwrap());
//        r_tilde.insert("1".to_string(), BigNumber::from_dec("7575191721496255329790454166600075461811327744716122725414003704363002865687003988444075479817517968742651133011723131465916075452356777073568785406106174349810313776328792235352103470770562831584011847").unwrap());
//        r_tilde.insert("2".to_string(), BigNumber::from_dec("7575191721496255329790454166600075461811327744716122725414003704363002865687003988444075479817517968742651133011723131465916075452356777073568785406106174349810313776328792235352103470770562831584011847").unwrap());
//        r_tilde.insert("0".to_string(), BigNumber::from_dec("7575191721496255329790454166600075461811327744716122725414003704363002865687003988444075479817517968742651133011723131465916075452356777073568785406106174349810313776328792235352103470770562831584011847").unwrap());
//        r_tilde.insert("DELTA".to_string(), BigNumber::from_dec("7575191721496255329790454166600075461811327744716122725414003704363002865687003988444075479817517968742651133011723131465916075452356777073568785406106174349810313776328792235352103470770562831584011847").unwrap());
//
//        let alpha_tilde = BigNumber::from_dec("15019832071918025992746443764672619814038193111378331515587108416842661492145380306078894142589602719572721868876278167686578705125701790763532708415180504799241968357487349133908918935916667492626745934151420791943681376124817051308074507483664691464171654649868050938558535412658082031636255658721308264295197092495486870266555635348911182100181878388728256154149188718706253259396012667950509304959158288841789791483411208523521415447630365867367726300467842829858413745535144815825801952910447948288047749122728907853947789264574578039991615261320141035427325207080621563365816477359968627596441227854436137047681372373555472236147836722255880181214889123172703767379416198854131024048095499109158532300492176958443747616386425935907770015072924926418668194296922541290395990933578000312885508514814484100785527174742772860178035596639").unwrap();
//        let predicate = predicate();
//
//        let mut t = HashMap::new();
//        t.insert("3".to_string(), BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap());
//        t.insert("1".to_string(), BigNumber::from_dec("42633794716405561166353758783443542082448925291459053109072523255543918476162700915813468558725428930654732720550388668689693688311928225615248227542838894861904877843723074396340940707779041622733024047596548590206852224857490474241304499513238502020545990648514598111266718428654653729661393150510227786297395151012680735494729670444556589448695350091598078767475426612902588875098609575406745197186551303270002056095805065181028711913238674710248448811408868490444106100385953490031500705851784934426334273103423243390196341490285527664863980694992161784435576660236953710046735477189662522764706620430688287285864").unwrap());
//        t.insert("2".to_string(), BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap());
//        t.insert("0".to_string(), BigNumber::from_dec("78330570979325941798365644373115445702503890126796448033540676436952642712474355493362616083006349657268453144498828167557958002187631433688600374998507190955348534609331062289505584464470965930026066960445862271919137219085035331183489708020179104768806542397317724245476749638435898286962686099614654775075210180478240806960936772266501650713946075532415486293498432032415822169972407762416677793858709680700551196367079406811614109643837625095590323201355832120222436221544300974405069957610226245036804939616341080518318062198049430554737724174625842765640174768911551668897074696860939233144184997614684980589924").unwrap());
//        t.insert("DELTA".to_string(), BigNumber::from_dec("55689486371095551191153293221620120399985911078762073609790094310886646953389020785947364735709221760939349576244277298015773664794725470336037959586509430339581241350326035321187900311380031369930812685369312069872023094452466688619635133201050270873513970497547720395196520621008569032923514500216567833262585947550373732948093781160931218148684610639834393439060745307992621402105096757255088629786888737281709324281552413987274960223110927132818654699339106642690418211294536451370321243108928564278387404368783012923356880461335644797776340191719071088431730682007888636922131293039620517120570619351490238276806").unwrap());
//
//        PrimaryPredicateGEInitProof {
//            c_list,
//            tau_list,
//            u,
//            u_tilde,
//            r,
//            r_tilde,
//            alpha_tilde,
//            predicate,
//            t
//        }
//    }
//
//    pub fn c_list() -> Vec<BigNumber> {
//        let mut c_list: Vec<BigNumber> = Vec::new();
//        c_list.push(BigNumber::from_dec("78330570979325941798365644373115445702503890126796448033540676436952642712474355493362616083006349657268453144498828167557958002187631433688600374998507190955348534609331062289505584464470965930026066960445862271919137219085035331183489708020179104768806542397317724245476749638435898286962686099614654775075210180478240806960936772266501650713946075532415486293498432032415822169972407762416677793858709680700551196367079406811614109643837625095590323201355832120222436221544300974405069957610226245036804939616341080518318062198049430554737724174625842765640174768911551668897074696860939233144184997614684980589924").unwrap());
//        c_list.push(BigNumber::from_dec("42633794716405561166353758783443542082448925291459053109072523255543918476162700915813468558725428930654732720550388668689693688311928225615248227542838894861904877843723074396340940707779041622733024047596548590206852224857490474241304499513238502020545990648514598111266718428654653729661393150510227786297395151012680735494729670444556589448695350091598078767475426612902588875098609575406745197186551303270002056095805065181028711913238674710248448811408868490444106100385953490031500705851784934426334273103423243390196341490285527664863980694992161784435576660236953710046735477189662522764706620430688287285864").unwrap());
//        c_list.push(BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap());
//        c_list.push(BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap());
//        c_list.push(BigNumber::from_dec("55689486371095551191153293221620120399985911078762073609790094310886646953389020785947364735709221760939349576244277298015773664794725470336037959586509430339581241350326035321187900311380031369930812685369312069872023094452466688619635133201050270873513970497547720395196520621008569032923514500216567833262585947550373732948093781160931218148684610639834393439060745307992621402105096757255088629786888737281709324281552413987274960223110927132818654699339106642690418211294536451370321243108928564278387404368783012923356880461335644797776340191719071088431730682007888636922131293039620517120570619351490238276806").unwrap());
//        c_list
//    }
//
//    pub fn tau_list() -> Vec<BigNumber> {
//        let mut tau_list: Vec<BigNumber> = Vec::new();
//        tau_list.push(BigNumber::from_dec("37691036678500088864090706889277344529085698202855318342609662324455534725777810174779988243614834740383002484042961779535438729512700925723800184769772855117653609397311580937440131814111009890073972276784593662470810723687676167680062717239972656425563430838236749325671702463390044920572001860955651242331741037260836613506653323682056706226370698422365916655999046380426509541586034749242827978969972239524676039139025602263974101808887008331192929679659076910995855665477952930199692854778469439162325030246066895851569630345729938981633504514117558420480144828304421708923356898912192737390539479512879411139535").unwrap());
//        tau_list.push(BigNumber::from_dec("37691036678500088864090706889277344529085698202855318342609662324455534725777810174779988243614834740383002484042961779535438729512700925723800184769772855117653609397311580937440131814111009890073972276784593662470810723687676167680062717239972656425563430838236749325671702463390044920572001860955651242331741037260836613506653323682056706226370698422365916655999046380426509541586034749242827978969972239524676039139025602263974101808887008331192929679659076910995855665477952930199692854778469439162325030246066895851569630345729938981633504514117558420480144828304421708923356898912192737390539479512879411139535").unwrap());
//        tau_list.push(BigNumber::from_dec("37691036678500088864090706889277344529085698202855318342609662324455534725777810174779988243614834740383002484042961779535438729512700925723800184769772855117653609397311580937440131814111009890073972276784593662470810723687676167680062717239972656425563430838236749325671702463390044920572001860955651242331741037260836613506653323682056706226370698422365916655999046380426509541586034749242827978969972239524676039139025602263974101808887008331192929679659076910995855665477952930199692854778469439162325030246066895851569630345729938981633504514117558420480144828304421708923356898912192737390539479512879411139535").unwrap());
//        tau_list.push(BigNumber::from_dec("37691036678500088864090706889277344529085698202855318342609662324455534725777810174779988243614834740383002484042961779535438729512700925723800184769772855117653609397311580937440131814111009890073972276784593662470810723687676167680062717239972656425563430838236749325671702463390044920572001860955651242331741037260836613506653323682056706226370698422365916655999046380426509541586034749242827978969972239524676039139025602263974101808887008331192929679659076910995855665477952930199692854778469439162325030246066895851569630345729938981633504514117558420480144828304421708923356898912192737390539479512879411139535").unwrap());
//        tau_list.push(BigNumber::from_dec("37691036678500088864090706889277344529085698202855318342609662324455534725777810174779988243614834740383002484042961779535438729512700925723800184769772855117653609397311580937440131814111009890073972276784593662470810723687676167680062717239972656425563430838236749325671702463390044920572001860955651242331741037260836613506653323682056706226370698422365916655999046380426509541586034749242827978969972239524676039139025602263974101808887008331192929679659076910995855665477952930199692854778469439162325030246066895851569630345729938981633504514117558420480144828304421708923356898912192737390539479512879411139535").unwrap());
//        tau_list.push(BigNumber::from_dec("47065304866607958075946961264533928435933122536016679690080278659386698316132559908768761685743414728586341914305025339970537873714845915164843100776821561200343390749927996265246866447155790487554483555192805709960222015718787293872197230832464704800887153568636866026153126587657548580608446574507279965440247754859129693686186427399103313737110632413255017522482016458190003045641077338674019608347139399755470654452373975228190041980152120799403855480909173865431397307988238759767251890853580982844825639097363091181044515877489450972963624109587697097258041963985607958610791800500711857115582406526050626576194").unwrap());
//        tau_list
//    }
//
//    pub fn mtilde() -> HashMap<String, BigNumber> {
//        let mut mtilde = HashMap::new();
//        mtilde.insert("height".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        mtilde.insert("age".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        mtilde.insert("sex".to_string(), BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap());
//        mtilde
//    }
//
    pub fn eq_proof() -> PrimaryEqualProof {
        let revealed_attrs = btreemap![
            "name".to_string() => BigNumber::from_dec("1139481716457488690172217916278103335").unwrap()
        ];

        let m = btreemap![
            "age".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126569555048377863338051254460267053606356944162460437192812434232786788496640641930").unwrap(),
            "sex".to_string() => BigNumber::from_dec("6461691768834933403326573210330277861354501442113655769882988760097155977792459796092706040876245423440766971450670662675952825317632013652532469629317617583714945063045022245480").unwrap(),
            "height".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126578939747270189080172212182414586274398455192612806812346160325332993411278449288").unwrap()
        ];
        PrimaryEqualProof {
            revealed_attrs,
            a_prime: BigNumber::from_dec("71198415862588101794999647637020594298636904952221229203758282286975648719760139091058820193148109269247332893072500009542535008873854752148253162724944592022459474653064164142982594342926411034455992098661321743462319688749656666526142178124484745737199241840970729963874025117751516490879240004090076615289806927701165887254974076649588902577976777511906325622743656262704616698456422853985442045734201762141883277985205745253481177231940188177322557579410753761153630309562334285168209207788901648739373257862961666829476892899815574748297248950737715666360295849203006237827045519446375662564835999315073290305487").unwrap(),
            e: BigNumber::from_dec("162083298053730499878539868675621169436369451197643364049023367091432000132455800020257076120708238452276269567894398158050524769029842452").unwrap(),
            v: BigNumber::from_dec("241132863422049783305938575438970984968886174719777147285301544874444739988430486983952995820260097840839069440378394346753879604392851933586337358561654582254955422523682312898019724436529062720828313475600193232595180909715451462600156154893650696125774460890921102329846137078074382707757647774945043683720900258432094924802673704740906359826573881239743758685541361718016925837587696092410894740410057107458758557447620998360525272595382152876024857626077651578395225059761356087394593446798198952106185244345409492051630909148188459601119687896316694712657729049955088211383277523909796073178401612271233635772969741130113707687830672377452944696453914387935655490225348165302887058751092766205283395163137925794804058129811328946552494253968865890668669461930678189893633436518087402186509936569439729722305359388858552931085264558311600831146504998575898993873331466073087451895775323562065569650308485086554411659").unwrap(),
            m,
            m2: BigNumber::from_dec("6461691768834933403326577886987794426564075567348355398300211701274010647148126371700410672728318280277334618630162515599217553371534203830461401099262597200716242881364353460506").unwrap(),
        }
    }

    pub fn aggregated_proof() -> AggregatedProof {
        AggregatedProof {
            c_list: vec![vec![1, 186, 92, 249, 189, 141, 143, 77, 171, 208, 34, 140, 90, 244, 94, 183, 45, 154, 176, 130, 60, 178, 12, 91, 106, 61, 126, 148, 197, 182, 25, 153, 96, 174, 3, 165, 20, 89, 43, 231, 112, 217, 35, 100, 69, 135, 47, 144, 253, 40, 158, 137, 14, 165, 152, 246, 60, 170, 0, 228, 18, 85, 19, 117, 184, 191, 8, 222, 140, 135, 204, 99, 152, 191, 200, 124, 95, 124, 138, 86, 120, 75, 160, 110, 21, 36, 100, 161, 60, 215, 45, 138, 147, 21, 211, 241, 40, 25, 98, 21, 41, 160, 115, 84, 184, 92, 113, 251, 138, 182, 201, 12, 42, 35, 243, 28, 13, 195, 2, 30, 119, 253, 227, 15, 51, 237, 221, 14, 193, 142, 152, 182, 63, 150, 188, 87, 216, 26, 201, 10, 166, 26, 223, 177, 210, 90, 123, 241, 43, 125, 37, 94, 48, 89, 240, 144, 246, 246, 202, 224, 86, 207, 134, 211, 140, 154, 77, 45, 168, 99, 192, 41, 142, 42, 106, 165, 64, 130, 26, 255, 247, 56, 250, 156, 193, 209, 139, 4, 234, 227, 138, 199, 99, 151, 3, 89, 46, 142, 137, 27, 152, 205, 147, 136, 121, 32, 126, 71, 112, 40, 0, 236, 30, 62, 12, 66, 74, 177, 19, 170, 170, 14, 149, 90, 43, 199, 68, 15, 239, 213, 131, 33, 112, 117, 13, 101, 181, 164, 202, 58, 143, 46, 105, 23, 171, 178, 36, 198, 189, 220, 128, 247, 59, 129, 189, 224, 171],
                         vec![2, 108, 127, 101, 174, 218, 32, 134, 244, 38, 234, 207, 183, 66, 169, 248, 220, 152, 219, 224, 147, 85, 180, 138, 119, 9, 112, 56, 171, 119, 32, 85, 150, 21, 32, 246, 205, 201, 127, 46, 230, 100, 227, 32, 121, 190, 24, 173, 28, 86, 154, 44, 66, 119, 101, 162, 138, 185, 201, 243, 172, 229, 25, 147, 210, 51, 172, 170, 113, 11, 245, 227, 33, 4, 197, 168, 253, 19, 136, 59, 158, 255, 53, 184, 168, 158, 46, 232, 119, 185, 114, 41, 17, 179, 201, 109, 92, 53, 238, 69, 40, 13, 2, 122, 179, 99, 68, 189, 76, 41, 105, 70, 85, 127, 150, 192, 111, 167, 53, 48, 221, 242, 243, 164, 202, 56, 243, 146, 104, 122, 12, 173, 136, 61, 169, 225, 79, 41, 180, 155, 198, 21, 192, 140, 223, 100, 207, 167, 50, 100, 17, 2, 102, 161, 47, 187, 96, 210, 156, 24, 214, 179, 43, 158, 9, 191, 186, 75, 40, 216, 47, 145, 104, 23, 8, 119, 90, 69, 104, 83, 183, 200, 85, 140, 134, 172, 12, 251, 73, 172, 157, 33, 100, 226, 180, 51, 102, 151, 36, 253, 149, 15, 97, 191, 210, 246, 28, 120, 161, 126, 51, 99, 181, 225, 54, 24, 131, 91, 178, 164, 116, 32, 67, 30, 181, 227, 245, 241, 172, 153, 113, 14, 127, 6, 98, 199, 250, 43, 119, 146, 160, 105, 138, 190, 162, 9, 230, 81, 116, 42, 31, 84, 160, 67, 219, 53, 100],
                         vec![1, 81, 185, 134, 123, 21, 18, 221, 49, 172, 39, 239, 236, 207, 16, 143, 240, 173, 88, 153, 7, 162, 166, 60, 151, 232, 163, 185, 151, 27, 178, 120, 14, 12, 201, 119, 144, 135, 130, 203, 231, 119, 46, 249, 128, 137, 136, 243, 91, 240, 120, 169, 203, 72, 35, 17, 151, 39, 246, 124, 44, 135, 141, 132, 178, 89, 195, 178, 253, 153, 216, 48, 226, 115, 1, 36, 137, 191, 159, 106, 192, 193, 254, 50, 97, 50, 204, 141, 202, 207, 8, 168, 100, 200, 247, 209, 198, 213, 58, 213, 202, 226, 82, 214, 206, 99, 143, 121, 91, 80, 19, 251, 59, 64, 79, 221, 234, 219, 244, 174, 44, 100, 141, 29, 163, 221, 175, 180, 131, 141, 42, 209, 0, 36, 199, 9, 10, 134, 93, 103, 96, 7, 11, 197, 228, 166, 132, 242, 31, 233, 228, 117, 242, 242, 64, 5, 21, 252, 184, 181, 124, 66, 168, 126, 165, 69, 30, 218, 112, 124, 134, 57, 143, 200, 9, 0, 71, 72, 251, 216, 5, 68, 126, 168, 209, 162, 147, 106, 245, 106, 240, 86, 56, 96, 124, 242, 119, 141, 132, 145, 104, 68, 224, 33, 61, 1, 16, 242, 210, 43, 56, 209, 209, 128, 200, 208, 54, 249, 111, 136, 246, 154, 105, 73, 64, 139, 81, 85, 177, 174, 214, 250, 59, 161, 159, 174, 38, 94, 195, 191, 120, 33, 69, 179, 235, 20, 106, 133, 209, 118, 61, 159, 242, 0, 101, 98, 104],
                         vec![1, 111, 80, 91, 53, 214, 139, 10, 197, 79, 134, 183, 50, 233, 244, 130, 80, 173, 167, 5, 130, 151, 183, 162, 97, 134, 246, 146, 37, 151, 103, 45, 68, 33, 204, 18, 157, 21, 98, 230, 225, 30, 162, 172, 75, 159, 115, 94, 72, 113, 153, 155, 117, 233, 95, 251, 29, 1, 149, 38, 117, 63, 112, 213, 48, 29, 3, 131, 238, 120, 48, 141, 105, 31, 127, 51, 176, 32, 203, 191, 155, 159, 91, 29, 87, 223, 30, 92, 146, 250, 182, 181, 155, 67, 253, 33, 165, 142, 195, 146, 180, 221, 83, 62, 46, 74, 29, 83, 175, 218, 132, 93, 42, 93, 105, 173, 189, 254, 193, 230, 113, 39, 45, 137, 143, 124, 190, 42, 19, 77, 13, 220, 137, 202, 128, 170, 10, 22, 37, 177, 200, 186, 3, 73, 171, 232, 81, 144, 36, 46, 70, 237, 208, 26, 84, 26, 141, 19, 37, 200, 83, 60, 27, 175, 96, 233, 246, 144, 137, 178, 140, 213, 13, 36, 137, 82, 107, 0, 239, 192, 187, 126, 20, 205, 40, 203, 33, 238, 88, 121, 132, 31, 87, 91, 65, 207, 144, 15, 249, 66, 58, 98, 64, 61, 236, 103, 203, 207, 20, 205, 48, 202, 247, 22, 248, 197, 188, 21, 178, 187, 193, 152, 164, 247, 53, 15, 33, 170, 145, 3, 213, 63, 205, 55, 158, 170, 62, 157, 207, 162, 117, 157, 215, 125, 94, 77, 251, 251, 25, 209, 207, 119, 16, 186, 210, 190, 83],
                         vec![1, 111, 80, 91, 53, 214, 139, 10, 197, 79, 134, 183, 50, 233, 244, 130, 80, 173, 167, 5, 130, 151, 183, 162, 97, 134, 246, 146, 37, 151, 103, 45, 68, 33, 204, 18, 157, 21, 98, 230, 225, 30, 162, 172, 75, 159, 115, 94, 72, 113, 153, 155, 117, 233, 95, 251, 29, 1, 149, 38, 117, 63, 112, 213, 48, 29, 3, 131, 238, 120, 48, 141, 105, 31, 127, 51, 176, 32, 203, 191, 155, 159, 91, 29, 87, 223, 30, 92, 146, 250, 182, 181, 155, 67, 253, 33, 165, 142, 195, 146, 180, 221, 83, 62, 46, 74, 29, 83, 175, 218, 132, 93, 42, 93, 105, 173, 189, 254, 193, 230, 113, 39, 45, 137, 143, 124, 190, 42, 19, 77, 13, 220, 137, 202, 128, 170, 10, 22, 37, 177, 200, 186, 3, 73, 171, 232, 81, 144, 36, 46, 70, 237, 208, 26, 84, 26, 141, 19, 37, 200, 83, 60, 27, 175, 96, 233, 246, 144, 137, 178, 140, 213, 13, 36, 137, 82, 107, 0, 239, 192, 187, 126, 20, 205, 40, 203, 33, 238, 88, 121, 132, 31, 87, 91, 65, 207, 144, 15, 249, 66, 58, 98, 64, 61, 236, 103, 203, 207, 20, 205, 48, 202, 247, 22, 248, 197, 188, 21, 178, 187, 193, 152, 164, 247, 53, 15, 33, 170, 145, 3, 213, 63, 205, 55, 158, 170, 62, 157, 207, 162, 117, 157, 215, 125, 94, 77, 251, 251, 25, 209, 207, 119, 16, 186, 210, 190, 83],
                         vec![1, 185, 37, 77, 23, 245, 214, 239, 127, 18, 101, 63, 229, 201, 171, 193, 32, 182, 124, 45, 15, 127, 58, 172, 226, 30, 246, 70, 33, 19, 117, 183, 29, 157, 209, 237, 41, 58, 208, 4, 105, 26, 73, 26, 69, 72, 21, 78, 106, 28, 72, 117, 102, 144, 199, 148, 3, 98, 81, 251, 246, 106, 50, 235, 129, 14, 186, 108, 216, 29, 41, 207, 233, 7, 179, 86, 224, 230, 187, 138, 125, 62, 68, 31, 66, 147, 205, 93, 100, 9, 134, 225, 210, 57, 36, 71, 134, 26, 179, 85, 37, 194, 32, 137, 91, 4, 91, 214, 220, 134, 173, 148, 14, 95, 209, 232, 79, 87, 12, 180, 217, 148, 240, 242, 190, 36, 229, 189, 16, 208, 75, 176, 153, 239, 212, 255, 45, 42, 250, 234, 139, 40, 104, 74, 21, 30, 184, 221, 126, 185, 23, 69, 114, 104, 249, 242, 248, 210, 97, 100, 141, 61, 176, 93, 200, 148, 152, 138, 31, 66, 99, 61, 237, 210, 42, 205, 60, 241, 92, 247, 1, 146, 203, 116, 237, 0, 171, 235, 250, 128, 74, 56, 223, 65, 189, 176, 91, 243, 174, 2, 111, 216, 233, 227, 28, 22, 41, 102, 225, 1, 21, 156, 212, 16, 243, 9, 94, 61, 246, 153, 193, 243, 188, 187, 154, 109, 168, 36, 89, 48, 236, 113, 74, 179, 158, 103, 51, 38, 15, 148, 18, 89, 218, 144, 71, 198, 8, 144, 104, 135, 160, 224, 98, 243, 106, 228, 198]],
            c_hash: BigNumber::from_dec("63841489063440422591549130255324272391231497635167479821265935688468807059914").unwrap()
        }
    }

    pub fn ge_proof() -> PrimaryPredicateGEProof {
        let m = btreemap![
            String::from("age") => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126569555048377863338051254460267053606356944162460437192812434232786788496640641930").unwrap(),
            String::from("sex") => BigNumber::from_dec("6461691768834933403326573210330277861354501442113655769882988760097155977792459796092706040876245423440766971450670662675952825317632013652532469629317617583714945063045022245480").unwrap(),
            String::from("height") => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126578939747270189080172212182414586274398455192612806812346160325332993411278449288").unwrap()
        ];

        let u = btreemap![
            "0".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567959011151277327486465732010670499547163375019558005816902584394576776464144080").unwrap(),
            "1".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567831328173150446641282633750159851002380912024287670857260052523199838850024252").unwrap(),
            "2".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap(),
            "3".to_string() => BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126567767486684087006218691084619904526729989680526652503377438786587511370042964338").unwrap()
        ];

        let r = btreemap![
            "0".to_string() => BigNumber::from_dec("122666581787896024104771761595539708848783314985870238259074669824520091098683817237172519182829174751114708491011709191270412318634809532273931666000301987869809614370778701672920770190235911538453236520585124998634470107126877826855765108565024357739461476219090897270520451817930736172663543943052827769367981507788289923500996293391654370634807890778790076616041326007628068206880269267272777192271905638118708385050200412890391080370252730064261452554932992620443959769478748678597670501698531981378757093642774169056547668193201752061644097178572361915153806621540894628974958162220867331621188215651633938457631228059207968660364669634554543579944958864314375144914088839439106378569969245085620007043098442351").unwrap(),
            "1".to_string() => BigNumber::from_dec("122666581787896024104771761595539708848783314985870238259074669824520091098683817237172519182829174751114708491011709191270412318634809532273931666000301987869809614370778701672920770190235911538453236520585124998634470107126877826855765108565024357739461476219090897270520451817930736172663543943052827769367981507788289923500996293391654370634807890778790076616041326007628068206880269267272777192271905638118708385050200412890391080370252730064261452554932992620443959769478748678597670501698531981378757093642774169056547668193201752061644097178572361915153806621540894628974958162220867331621188215651633938457631228059207968660364669634554543579944958864314375144914088839439106378569969245085620007043098442351").unwrap(),
            "2".to_string() => BigNumber::from_dec("122666581787896024104771761595539708848783314985870238259074669824520091098683817237172519182829174751114708491011709191270412318634809532273931666000301987869809614370778701672920770190235911538453236520585124998634470107126877826855765108565024357739461476219090897270520451817930736172663543943052827769367981507788289923500996293391654370634807890778790076616041326007628068206880269267272777192271905638118708385050200412890391080370252730064261452554932992620443959769478748678597670501698531981378757093642774169056547668193201752061644097178572361915153806621540894628974958162220867331621188215651633938457631228059207968660364669634554543579944958864314375144914088839439106378569969245085620007043098442351").unwrap(),
            "3".to_string() => BigNumber::from_dec("122666581787896024104771761595539708848783314985870238259074669824520091098683817237172519182829174751114708491011709191270412318634809532273931666000301987869809614370778701672920770190235911538453236520585124998634470107126877826855765108565024357739461476219090897270520451817930736172663543943052827769367981507788289923500996293391654370634807890778790076616041326007628068206880269267272777192271905638118708385050200412890391080370252730064261452554932992620443959769478748678597670501698531981378757093642774169056547668193201752061644097178572361915153806621540894628974958162220867331621188215651633938457631228059207968660364669634554543579944958864314375144914088839439106378569969245085620007043098442351").unwrap(),
            "DELTA".to_string() => BigNumber::from_dec("122666581787896024104771761595539708848783314985870238259074669824520091098683817237172519182829174751114708491011709191270412318634809532273931666000301987869809614370778701672920770190235911538453236520585124998634470107126877826855765108565024357739461476219090897270520451817930736172663543943052827769367981507788289923500996293391654370634807890778790076616041326007628068206880269267272777192271905638118708385050200412890391080370252730064261452554932992620443959769478748678597670501698531981378757093642774169056547668193201752061644097178572361915153806621540894628974958162220867331621188215651633938457631228059207968660364669634554543579944958864314375144914088839439106378569969245085620007043098442351").unwrap()
        ];

        let t = btreemap![
            "0".to_string() => BigNumber::from_dec("78330570979325941798365644373115445702503890126796448033540676436952642712474355493362616083006349657268453144498828167557958002187631433688600374998507190955348534609331062289505584464470965930026066960445862271919137219085035331183489708020179104768806542397317724245476749638435898286962686099614654775075210180478240806960936772266501650713946075532415486293498432032415822169972407762416677793858709680700551196367079406811614109643837625095590323201355832120222436221544300974405069957610226245036804939616341080518318062198049430554737724174625842765640174768911551668897074696860939233144184997614684980589924").unwrap(),
            "1".to_string() => BigNumber::from_dec("42633794716405561166353758783443542082448925291459053109072523255543918476162700915813468558725428930654732720550388668689693688311928225615248227542838894861904877843723074396340940707779041622733024047596548590206852224857490474241304499513238502020545990648514598111266718428654653729661393150510227786297395151012680735494729670444556589448695350091598078767475426612902588875098609575406745197186551303270002056095805065181028711913238674710248448811408868490444106100385953490031500705851784934426334273103423243390196341490285527664863980694992161784435576660236953710046735477189662522764706620430688287285864").unwrap(),
            "2".to_string() => BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap(),
            "3".to_string() => BigNumber::from_dec("46369083086117629643055653975857627769028160828983987182078946658047913327657659075673217449651724551898727205835194812073207899212452294564444639346668484070129687160427147938076018605551830861026465851076491021338935906152700477977234743314769181602525430955162020248817746661022702546242365043781931307417744503802184994273068810023321000162105949048577491174537385619391992689890177380388187493777623608221690561227863928538947292434940859766215223694325554781311625439704847971277102325299579636232682943235572924328291095040633959587110788517670425708774447736335155403676598370782714048226320498065574125026899").unwrap(),
            "DELTA".to_string() => BigNumber::from_dec("55689486371095551191153293221620120399985911078762073609790094310886646953389020785947364735709221760939349576244277298015773664794725470336037959586509430339581241350326035321187900311380031369930812685369312069872023094452466688619635133201050270873513970497547720395196520621008569032923514500216567833262585947550373732948093781160931218148684610639834393439060745307992621402105096757255088629786888737281709324281552413987274960223110927132818654699339106642690418211294536451370321243108928564278387404368783012923356880461335644797776340191719071088431730682007888636922131293039620517120570619351490238276806").unwrap()
        ];

        PrimaryPredicateGEProof {
            u,
            r,
            mj: BigNumber::from_dec("6461691768834933403326572830814516653957231030793837560544354737855803497655300429843454445497126569555048377863338051254460267053606356944162460437192812434232786788496640641930").unwrap(),
            alpha: BigNumber::from_dec("15019832071918025992746443764672619814038193111378331515587108416842661492145380306078894142589602719572721868876278167686210705380338102691218393130393885672695618412529738419131694926443107219330694482439903234395193851871472925835039379909853454508267226053046255940557629449048653188523919553702545953724489357880127160704800260353007771778801244908160960828454115645487868830738739976138947949505366080323799159654252725215417470924265496096864737420292879717953990073198774585977677974887563743667406941320910576277132072350218452884841014022648967794316567016887837205701017499498636748288004981818643125542585776429419200955219536940661401665401273238350271276070084547091903752551649057233346746822426635975545515195870976674441104284294336189831971933619615980881781820696853193401192672937826151341781675749898224527543492305127").unwrap(),
            t,
            predicate: predicate()
        }
    }
//
//    pub fn primary_proof() -> PrimaryProof {
//        PrimaryProof {
//            eq_proof: eq_proof(),
//            ge_proofs: vec![ge_proof()]
//        }
//    }
//
//    pub fn sub_proof_request() -> SubProofRequest {
//        let mut sub_proof_request_builder = SubProofRequestBuilder::new().unwrap();
//        sub_proof_request_builder.add_revealed_attr("name").unwrap();
//        sub_proof_request_builder.add_predicate("age", "GE", 18).unwrap();
//        sub_proof_request_builder.finalize().unwrap()
//    }
//
//    pub fn revealed_attrs() -> HashSet<String> {
//        HashSet::from_iter(vec!["name".to_owned()].into_iter())
//    }
//
//    pub fn unrevealed_attrs() -> HashSet<String> {
//        HashSet::from_iter(vec!["height".to_owned(), "age".to_owned(), "sex".to_owned()])
//    }
//
//    pub fn credential_revealed_attributes_values() -> CredentialValues {
//        let mut credential_values_builder = CredentialValuesBuilder::new().unwrap();
//        credential_values_builder.add_value("name", "1139481716457488690172217916278103335").unwrap();
//        credential_values_builder.finalize().unwrap()
//    }
//
    pub fn predicate() -> Predicate {
        Predicate {
            attr_name: "age".to_owned(),
            p_type: PredicateType::GE,
            value: 18
        }
    }
}
