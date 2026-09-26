//! Interop TKLV contínua — CI gate byte-exato (s410i).
//!
//! Contrato: ADR-0004 (repo comunitário) ↔ ADR-0063 (OS). O codec canônico é
//! `k_nano::storage::tickv` (R0, fonte da verdade); `neural-sgdb::tickv` é o
//! port documentado byte-exato. Estes testes rodam no CI nas DUAS direções:
//!
//! 1. **OS gera → NSGDB lê:** volume escrito pelo `TickvLite` real (RamFlash)
//!    + records sintéticos → `neural_sgdb::tickv::scan_volume` reconstrói o
//!    mesmo mapa (map/offsets/append_off/corrupt/truncated).
//! 2. **NSGDB gera → OS lê:** records do `neural_sgdb::tickv::encode_record`
//!    → `k_nano::storage::scan_volume` reconstrói o mesmo mapa.
//! 3. **Golden cross de encoder:** os dois `encode_record` produzem bytes
//!    IDÊNTICOS para o mesmo vetor de casos (incl. >512, tombstone vlen=0).
//! 4. **Paridade de scan:** tombstone in-place (`TKL\0`), last-wins,
//!    tombstone por append, record corrompido e checkpoint TKCK fora do map
//!    → mesmos veredictos nos dois scanners.
//!
//! Se um lado mudar o formato e o outro não, o CI quebra aqui — a interop
//! nunca degrada silenciosamente (lição SESSION_410: dual-truth exige teste
//! contínuo, não promessa de doc).

#[cfg(test)]
mod tests {
    use k_nano::storage as os_st;
    use neural_sgdb::tickv as ns_tk;

    /// Statics FLASH/TICKV globais — serializa (padrão SESSION_346/368).
    static TEST_LOCK: spin::Mutex<()> = spin::Mutex::new(());

    /// Vetor de casos cross-encoder: key, value (vazio = tombstone shape).
    const CASES: &[(&str, &[u8])] = &[
        ("k", b"v"),
        ("md/L1/last_user", b"ola mundo"),
        (
            "md/L4/emb1",
            &[0u8, 255, 7, 42, 1, 2, 3, 4, 5, 6, 7, 8],
        ),
        ("hanr/user", b""),
        (
            "long/key/exceeds/one/sector/when/combined/with/value/padding",
            &[0xABu8; 700],
        ),
    ];

    fn reset_storage() {
        *os_st::TICKV.lock() = None;
        *os_st::FLASH.lock() = None;
    }

    /// CRC32 IEEE do neural-sgdb — reconstrução local do body key‖val
    /// (a função `crc32` do crate é pub em `neural_sgdb::storage`).
    fn ns_crc32(key: &[u8], val: &[u8]) -> u32 {
        let mut body = Vec::with_capacity(key.len() + val.len());
        body.extend_from_slice(key);
        body.extend_from_slice(val);
        neural_sgdb::storage::crc32(&body)
    }

    /// Volume real gerado pelo TickvLite do OS (RamFlash) — puts, overwrite
    /// (last-wins), checkpoint TKCK sintético e cauda apagada.
    fn os_generated_volume() -> Vec<u8> {
        reset_storage();
        os_st::install_ram_flash(64 * 1024);
        {
            let mut kv = os_st::TickvLite::new();
            kv.mount().expect("tickv mount");
            kv.put("md/L1/last_user", b"ola mundo").expect("put 1");
            kv.put("md/L2/last_asst", b"pong").expect("put 2");
            kv.put("md/L1/last_user", b"segundo turno").expect("overwrite");
            kv.put("hw/cpu/isa", b"x86_64").expect("put 3");
            let _ = kv.append_off();
        }
        let mut vol = os_st::dump_flash(64 * 1024).expect("dump flash");
        reset_storage();
        // Checkpoint TKCK sintético (mesma key canônica; ambos os scanners
        // devem omiti-la do map — é metadado, não memória).
        let ckpt_val = ns_tk::encode_ckpt(vol.len() as u64 + 512, &[
            (String::from("md/L1/last_user"), 512),
            (String::from("md/L2/last_asst"), 1024),
        ]);
        vol.truncate(vol.len()); // (dump já termina na região apagada)
        // TKCK vai ANTES da cauda apagada: reconstrói volume = records vivos
        // até append_off + TKCK + zeros. Como o dump já é 64KB inteiro com
        // zeros no fim, sobrescrevemos a primeira janela livre de 512 com o
        // record TKCK (o scanner do TickvLite real faz o mesmo no write_ckpt).
        let free = {
            let scan = os_st::scan_volume(&vol);
            scan.append_off as usize
        };
        let ckpt_rec = os_st::encode_record(os_st::CKPT_KEY.as_bytes(), &ckpt_val);
        assert_eq!(ckpt_rec.len(), 512);
        vol[free..free + 512].copy_from_slice(&ckpt_rec);
        vol
    }

    /// (1) OS gera → NSGDB lê: mapa byte-exato.
    #[test]
    fn os_volume_decoded_by_neural_sgdb() {
        let _g = TEST_LOCK.lock();
        let vol = os_generated_volume();
        let scan = ns_tk::scan_volume(&vol);
        assert_eq!(scan.corrupt, 0, "NSGDB viu corrupção em volume do OS");
        assert!(!scan.truncated);
        assert_eq!(
            scan.map.get("md/L1/last_user").map(|v| v.as_slice()),
            Some(&b"segundo turno"[..]),
            "last-wins quebrado no leitor NSGDB"
        );
        assert_eq!(
            scan.map.get("md/L2/last_asst").map(|v| v.as_slice()),
            Some(&b"pong"[..])
        );
        assert_eq!(
            scan.map.get("hw/cpu/isa").map(|v| v.as_slice()),
            Some(&b"x86_64"[..])
        );
        assert!(
            !scan.map.contains_key(os_st::CKPT_KEY),
            "TKCK vazou como memória no leitor NSGDB"
        );
        assert!(scan.map.len() >= 3);
        assert!(scan.append_off > 0);
        // Cross-check: o scanner do OS vê exatamente o mesmo volume.
        let scan_os = os_st::scan_volume(&vol);
        assert_eq!(scan_os.map, scan.map, "scanners divergem no map");
        assert_eq!(scan_os.offsets, scan.offsets, "scanners divergem nos offsets");
        assert_eq!(scan_os.append_off, scan.append_off);
        assert_eq!(scan_os.corrupt, scan.corrupt);
        assert_eq!(scan_os.truncated, scan.truncated);
    }

    /// (2) NSGDB gera → OS lê: mapa byte-exato (direção reversa).
    #[test]
    fn nsgdb_volume_decoded_by_os() {
        let _g = TEST_LOCK.lock();
        let mut vol = Vec::new();
        for (k, v) in CASES {
            vol.extend_from_slice(&ns_tk::encode_record(k.as_bytes(), v));
        }
        // Tombstone por append (vlen=0) do leitor neural-sgdb.
        vol.extend_from_slice(&ns_tk::encode_record(b"md/L4/emb1", b""));
        let scan = os_st::scan_volume(&vol);
        assert_eq!(scan.corrupt, 0, "OS viu corrupção em volume do NSGDB");
        assert!(!scan.truncated);
        assert_eq!(
            scan.map.get("md/L1/last_user").map(|v| v.as_slice()),
            Some(&b"ola mundo"[..])
        );
        assert_eq!(
            scan.map.get("long/key/exceeds/one/sector/when/combined/with/value/padding")
                .map(|v| v.as_slice()),
            Some(&[0xABu8; 700][..])
        );
        assert!(
            !scan.map.contains_key("md/L4/emb1"),
            "tombstone vlen=0 não foi aplicado pelo leitor do OS"
        );
        // hanr/user já entra como tombstone (vlen=0) no CASES.
        assert!(!scan.map.contains_key("hanr/user"));
        assert_eq!(scan.map.len(), 3);
        // Cross-check: o scanner do NSGDB vê exatamente o mesmo volume.
        let scan_ns = ns_tk::scan_volume(&vol);
        assert_eq!(scan_ns.map, scan.map);
        assert_eq!(scan_ns.offsets, scan.offsets);
        assert_eq!(scan_ns.append_off, scan.append_off);
    }

    /// (3) Golden cross de encoder: bytes IDÊNTICOS nos dois codecs.
    #[test]
    fn encode_record_byte_exact_cross() {
        for (k, v) in CASES {
            let a = os_st::encode_record(k.as_bytes(), v);
            let b = ns_tk::encode_record(k.as_bytes(), v);
            assert_eq!(
                a, b,
                "encode_record divergente para key={} (vlen={})",
                k,
                v.len()
            );
            assert_eq!(a.len() % 512, 0);
            assert_eq!(a.len(), os_st::record_size(k.len(), v.len()));
            assert_eq!(a.len(), ns_tk::record_size(k.len(), v.len()));
            // Header canônico: magic + lens + crc só sobre key‖val.
            assert_eq!(&a[0..4], b"TKLV");
            assert_eq!(&a[4..8], &(k.len() as u32).to_le_bytes());
            assert_eq!(&a[8..12], &(v.len() as u32).to_le_bytes());
            // CRC32 (IEEE) cobre somente key‖val — validado nos DOIS lados.
            let body_len = k.len() + v.len();
            let crc_field = u32::from_le_bytes(a[12..16].try_into().unwrap());
            assert_eq!(crc_field, os_st::crc32(&a[16..16 + body_len]));
            assert_eq!(crc_field, ns_crc32(k.as_bytes(), v));
        }
        // Tombstone por append também é byte-exato.
        assert_eq!(
            os_st::encode_record(b"dead/key", b""),
            ns_tk::encode_record(b"dead/key", b"")
        );
    }

    /// (4) Paridade de scan: tombstone in-place (`TKL\0`), corrupt CRC.
    #[test]
    fn scan_parity_tombstone_inplace_and_corrupt() {
        let _g = TEST_LOCK.lock();
        let mut vol = Vec::new();
        vol.extend_from_slice(&os_st::encode_record(b"alive", b"1"));
        vol.extend_from_slice(&os_st::encode_record(b"deleted", b"bye"));
        // Tombstone in-place: magic[3] 'V'→0, resto preservado.
        let start = vol.len();
        vol.extend_from_slice(&os_st::encode_record(b"deleted", b"bye2"));
        vol[start + 3] = 0x00;
        // Record corrompido: byte do body alterado → CRC falha.
        let cstart = vol.len();
        vol.extend_from_slice(&os_st::encode_record(b"broken", b"data"));
        let body_i = cstart + os_st::HEADER + 6; // dentro de "data"
        vol[body_i] ^= 0xFF;

        let a = os_st::scan_volume(&vol);
        let b = ns_tk::scan_volume(&vol);
        assert_eq!(a.map, b.map);
        assert_eq!(a.offsets, b.offsets);
        assert_eq!(a.append_off, b.append_off);
        assert_eq!(a.corrupt, b.corrupt, "contador de corrupt divergiu");
        assert_eq!(a.truncated, b.truncated);
        // Semântica: tombstone in-place invalida só o RECORD (V=0) — a versão
        // anterior do mesmo key continua viva (paridade invalidate_key do OS);
        // record corrupt não indexa.
        assert_eq!(
            a.map.get("deleted").map(|v| v.as_slice()),
            Some(&b"bye"[..]),
            "tombstone in-place deve matar a versão, não a key"
        );
        assert!(!a.map.contains_key("broken"));
        assert_eq!(a.map.get("alive").map(|v| v.as_slice()), Some(&b"1"[..]));
        assert_eq!(a.corrupt, 1);
    }
}
