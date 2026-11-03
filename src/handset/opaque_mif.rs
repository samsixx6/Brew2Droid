//! Fatos de abertura de pares MOD/MIF opacos verificados, sem alterar arquivos.
//!
//! As classes/importações foram conferidas em MIFs legíveis de executáveis com
//! SHA256 idêntico e recursos coincidentes. O arquivo opaco permanece original;
//! esta tabela não o decripta nem oferece seus textos/imagens. Hash do executável
//! sozinho não basta: bootstraps iguais podem pertencer a releases distintos.

pub(super) struct Identity {
    module: &'static str,
    mif: &'static str,
    pub class: u32,
    pub dependencies: &'static [u32],
}

const IDENTITIES: &[Identity] = &[
    // asphalt3
    Identity {
        module: "94e7329d73313e7af041dd302b7b7edfdd6d5ea9b54c6881f6bbbeddbb1456e1",
        mif: "07f70ae6223c09d9d9e7b80099662b6db3ba51721e4188816865f81507b74b93",
        class: 0x0105b08c, dependencies: &[],
    },
    // assassinscreed2_3d
    Identity {
        module: "81cec20a2c65d3bbfa7ee85cb976105fa4f8ed423c9cd35d8eed18eecf935110",
        mif: "d13108de62d27606f851ad32fdba50e6539f4eae7bf0f5655c3f165b062d90d5",
        class: 0x010911f2, dependencies: &[],
    },
    // avp3d
    Identity {
        module: "b067493946b02858af1ea7ae046c7f77547e5f4d9d563da3dc59989a394c5fcf",
        mif: "90ca067a6e8b0a660859410bfff42f6588786d596950746ff7018e50fb1b5cb6",
        class: 0x01034789, dependencies: &[0x0102ebda],
    },
    // bio3d
    Identity {
        module: "6dfa96404ddfd8426be183520603961536639e9b2fa0fa9a8718740a4b033d20",
        mif: "5dbe7712affe512b7f0d0f2f0a7f3b82e1bb7a7f2fcea6bb2ba09cd7b7dca95c",
        class: 0x0109005b, dependencies: &[],
    },
    // dmc_mas
    Identity {
        module: "17e83fa747a0045ca4ed29a99d8eb927d373ade38ba71f415378e161ab6e6ee0",
        mif: "ff096e9a19fadebe06626d0b4e8d581bca3ef3096314cd7b79d2b913cc37a2e3",
        class: 0x01070e36, dependencies: &[0x010292c3],
    },
    // doom2rpg
    Identity {
        module: "3430cfc11fc1c6872987783c6078f7d8e78f3424a273135489c8684b5ca9f38a",
        mif: "65312b4431586f8a50370b771640188b49133dda64f15780bf17dac3fa52361f",
        class: 0x0109010f, dependencies: &[],
    },
    // doomrpg
    Identity {
        module: "a41e79f3c735d1151604a7c4a3d4381b9a1e800cf2f0471ce1561321c987198c",
        mif: "198e02a6914630a81d386d2a7187a66534cba1c00bccb199592d4b3ab460ae83",
        class: 0x01035893, dependencies: &[],
    },
    // ducati_3d
    Identity {
        module: "33f45ce215706e21b52d41f90bca0e3e27f9ab14c0a96b35d66aac7a77482744",
        mif: "343d69b1395659fd0f01da83d474440360651768bf123f4207966e764de8c037",
        class: 0x01039966, dependencies: &[0x0102ebda],
    },
    // duke3d
    Identity {
        module: "0931f7b466be05d3bee453caf30fc164ff06742bac3bc1ac664d31df5447abab",
        mif: "592f5d105169dafea59ad3b67c2a10755ed25bfd4e446a8a189cd568d18fbfac",
        class: 0x01034b16, dependencies: &[0x010292c3],
    },
    // dukemp
    Identity {
        module: "e43aaab1422153cbdc40648e94427da53e9a2b235e3d7fe341958b7dde69f84f",
        mif: "0c4f9c5d343329f2d566201aa0dc31a06977f0bee51977249164064a38764598",
        class: 0x010b88f1, dependencies: &[0x01062356],
    },
    // ff53d
    Identity {
        module: "045c62dcceffa16e5e02e68bdf87b0da1741a5aaff5974eea9fa21a1a4a23ea5",
        mif: "2b1be647843f81e594ffc076356d1adb3baae6e1fea0d551c575d57006a840fb",
        class: 0x01072750, dependencies: &[],
    },
    // fox_moto_3d
    Identity {
        module: "f03ee2457457cf9d4cfa4c7d32555a19218d5ec400c335a31efb1049b87ac9fd",
        mif: "28c3dfb14ee9990d5964c0afaf7947789262dc4924259eed1c6db4b61c4fcd8e",
        class: 0x01039a81, dependencies: &[0x0102ebda],
    },
    // gof
    Identity {
        module: "7b0cfcba287c5de819718631aed5ec66b3fa3b7ec7c4be5fc834aaa202ba40a8",
        mif: "0baa45c46415014be5e094f3c7e8093c6020a4469a0a3bb31663cd21fed20ed6",
        class: 0x62edaeba, dependencies: &[],
    },
    // helistrike
    Identity {
        module: "a8464e32b8450f0fb60872ff3a2d93aa5e69fb3277d54f896e09847055cb0276",
        mif: "145bb15e1256451b9f893f4a7d693d39528f24f552ef4bc613d4f4eb46752dda",
        class: 0x12324eab, dependencies: &[],
    },
    // nascar09
    Identity {
        module: "1e0ce36709459c58a80c78cd86f4e51f12aa45496d1720ac2939c8c82a031f32",
        mif: "0ce212b7ad7117e3d9c610189323837339e639556dc7c712e24697f88537e70a",
        class: 0x010751a5, dependencies: &[],
    },
    // nfs09
    Identity {
        module: "2876d56e6c20d911ab094850767860e4c1845f06c6fd51a08076bd296b704742",
        mif: "5e1c5b4a958f0a82095b46d86b75c734ec9424444f6e0ec600de198b27df4d35",
        class: 0x01074912, dependencies: &[],
    },
    // nfs10
    Identity {
        module: "cf47cbcfda63dd6476710014afd17ad9e6cc0be40341cfbc8f22c129f4ca0495",
        mif: "1d0bf1d0217484d7ace330ba080c1a7928d07a2ee0d00116c5df1d93fa9b71ad",
        class: 0x0109008d, dependencies: &[],
    },
    // nfs11
    Identity {
        module: "d0bb00ffdf90f83648e2fafd61616d31c5856ab5ad1b9aacce38886ae9c98540",
        mif: "e82713b585e351dd9a5696089927af052993677c98a2ec758eba540f0a9cfc72",
        class: 0x0109f7c4, dependencies: &[],
    },
    // nfs12
    Identity {
        module: "cb5042c2b27b5935f8b191f7e4304e7287a5247ebffd61cbe94c8e6ef597f347",
        mif: "e4d1c335b89a39bcd5ae5b27dd70b4c9a475801cb5c710aab3be95b4a98d3efd",
        class: 0x010fa44d, dependencies: &[],
    },
    // nfsps
    Identity {
        module: "863c041472d108b6c1edee37971e246640c52f6d7684f80a9d33bf9d47929dfe",
        mif: "8d24db076e85499754f7311ffbabd410b6faa5a0643efb3077967d92f1805c54",
        class: 0x01061194, dependencies: &[],
    },
    // orcsandelves2
    Identity {
        module: "1b8664e937ef8c43c474dff9b82c398b46425c5cca43eff5919e4ce96fc3d750",
        mif: "bfa373a3837e32a7d86a590fddb427a8e52ce184809e6b64305606c9b6fa0dc0",
        class: 0x010619c3, dependencies: &[],
    },
    // predator3d
    Identity {
        module: "1373d44fd4d6c5364ba8232ae3e76e0ba4b7fbe1772e806de0188b8f32d04d5e",
        mif: "63de00f2d24b5eee28b316d43186da4db9373b620315f7765def20078fa2c8f8",
        class: 0x01085689, dependencies: &[0x010292c3],
    },
    // tekken2
    Identity {
        module: "f9ae09cec286a4524ec707c54e80555a47774c5b737e1f28c5d5a87515cc6204",
        mif: "3c0ac2a7b66a14a0c2d87430b04c7f2bdd33bddddaba6eeb385561e20f0ec2f0",
        class: 0x010882b9, dependencies: &[0x010292c3],
    },
    // wolfensteinrpg
    Identity {
        module: "28e9bb8625acab595eb08cfb91792d45d152c1ef4790a9eba162704b20a69606",
        mif: "b24278a677fd9bfab7fefc4c0a1a89874a555d6de606d4858c3845751c6323ee",
        class: 0x01081949, dependencies: &[],
    },
];

pub(super) fn knows_mif(hash: &str) -> bool {
    IDENTITIES.iter().any(|entry| entry.mif == hash)
}

pub(super) fn identity(module: &str, mif: &str) -> Option<&'static Identity> {
    IDENTITIES.iter().find(|entry| entry.module == module && entry.mif == mif)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn neither_known_hash_alone_can_identify_a_pair() {
        for (i, entry) in IDENTITIES.iter().enumerate() {
            assert_eq!(identity(entry.module, entry.mif).unwrap().class, entry.class);
            assert!(knows_mif(entry.mif));
            assert!(identity(entry.module, "unknown").is_none());
            assert!(identity("unknown", entry.mif).is_none());
            for (j, other) in IDENTITIES.iter().enumerate() {
                if i != j { assert!(identity(entry.module, other.mif).is_none()); }
            }
        }
        assert!(!knows_mif("unknown"));
    }
    #[test]
    fn identities_are_distinct_complete_hashes_and_have_no_zero_class() {
        let mut pairs=std::collections::BTreeSet::new();
        for entry in IDENTITIES {
            assert!(pairs.insert((entry.module,entry.mif)));
            for hash in [entry.module,entry.mif] {
                assert_eq!(hash.len(),64);
                assert!(hash.bytes().all(|c|c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            }
            assert_ne!(entry.class,0);
            assert!(entry.dependencies.iter().all(|c|*c!=0 && *c!=entry.class));
        }
        assert_eq!(pairs.len(),24);
    }
}
