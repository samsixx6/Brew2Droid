//! Geometrias compatíveis observadas na auditoria local de inicialização de 2026-10-29.
//! São impressões digitais e dimensões; nenhum recurso dos jogos faz parte do catálogo.
//! A observação de um menu não comprova jogabilidade nem o hardware original do aparelho.
use super::super::{mif_lookup::Index, types::DisplayGeometry};
use std::io::Read;
use std::path::Path;

const MAX_MIF: u64 = 1024 * 1024;
struct Profile {
    module: &'static str,
    mif: &'static str,
    width: u16,
    height: u16,
}

// O par é necessário: um mesmo código pode ter versões de MIF e telas diferentes.
// Os dois registros de Fruit Escape têm o mesmo par e compartilham uma entrada.
const PROFILES: &[Profile] = &[
    // game-0026: os getters originais 0x1000baac/0x1000baa4 retornam 320/480;
    // 0x1000384c monta o AEERect completo com essas dimensões. O RLB também
    // precisa ser o original: o MOD é apenas o carregador. Tabuleiro e tutorial
    // completos foram conferidos em execução; o override manual tem prioridade.
    Profile {
        module: "e5061cfbc69ee8eea080a9b15809092c74d5601d374a2df417384f6bf6440200",
        mif: "d9089eef74d9e339328df96efad1f244a8b2f4b1155d54686eb00af17d3cbee0",
        width: 320,
        height: 480,
    },
    // game-0367: viewport compativel, nao uma identificacao de LCD historico.
    // Categoria compilada146, body120x134+footer120x12; retangulos e navegacao
    // observados sem leitura fora dos recursos. O MBR original tambem e exigido.
    Profile {
        module: "9bc21120c00d32dc53f30bcc4fc09d381af60e6cc9e854d9ead7e45cd377ec0c",
        mif: "b553d4da8e10a279162e7d6cb74eaaff29ca26b7eda17a1cdbcd8c29cd3bf434",
        width: 120,
        height: 146,
    },
    // game-0514: 1fbb0/1fbc0 limitam o viewport completo a120x146; os retângulos
    // em1fc24..1fc44 usam esses limites. O cache original tem linhas separadas
    // por240bytes (RGB565 de120pixels). Perfil compatível, sem identificar hardware.
    Profile {
        module: "6c655333625da5b0f0ef78bff6ec23533572fb69f695afa3c756b885bef8577d",
        mif: "1ee6a767df2833f4139ea3789d0be8507c34a8f5b7242d13733e8ec7fb7279fe",
        width:120, height:146,
    },

    // game-0073: o fallback 240x320 excede o buffer de assets do build VX4400.
    // Menu e áudio observados durante 20 s em 128x160; não é prova de gameplay.
    Profile {
        module: "188ad3140e5a379cb69853369124153262b7311fcf32125e3f335aec4a6af0c5",
        mif: "83ecbd956a34dd7bb42efed1ef3d5055503c56c0727a04c0caeadca50bd32fa0",
        width: 128,
        height: 160,
    },
    // game-0099 / game-0100.
    Profile {
        module: "ec6468e48bac24002744577387db4036f4e877fa5cc4d23b250fceacb935273a",
        mif: "cd917c2c0c359cfc970d578a0a3fb2195df22c5471cc76c9fc25b6e308f0a127",
        width: 176,
        height: 220,
    },
    // game-0108.
    Profile {
        module: "88c061c177a7ac66e26289b099b7c0df8ba82fa8acc4f3218264afa427d7ad03",
        mif: "65b92295887d4ecfa4cea4160f2c9c94cddf907d61ba41991588fe8d6efec737",
        width: 240,
        height: 292,
    },
    // game-0154.
    Profile {
        module: "80a99bfd8e759dbe85c7ed1497949876cb7a6121321901d105b0942bbb36b9c3",
        mif: "eca9b7a89248ada441cf41d38aed0872f1e3afb7bfe0d2386f02a71c2dddc593",
        width: 320,
        height: 240,
    },
    // game-0160: a mensagem de conexão e o mapa ficam íntegros em 176x220.
    Profile {
        module: "068c6738d3199ac483b94e4ce6c0bfc8e531d1d1b5b80617b8cdd9f6accade2d",
        mif: "fa0175038f8e4f3ea5885e8e671ea49c22891a82a449b661c263d31c7836cc37",
        width: 176,
        height: 220,
    },
    // game-0181: fundo 176x206 apresentado em y=1; canvas compatível 176x208.
    Profile {
        module: "be4645dae81389516f885c87134c7dc77d97ab0ba6e53455043d40b287383cf8",
        mif: "24956835aaad00f6527a088c8b579b71a415e09261334ea3cc741bb842321afc",
        width: 176,
        height: 208,
    },
    // game-0235.
    Profile {
        module: "02c90085dfff1b7a5e5957d51be23ba6d726aea5e82f47788ea1aa353a896a2f",
        mif: "269aefb161cfff76d5f21ac43aa1df10e0dd7ea9b2463fd2d33efb9654c6bdf7",
        width: 120,
        height: 160,
    },
    // game-0332: largura <=125 seleciona o título original 120x143 (0x16e70).
    // Em 120x146 o recorte é (0,0,120,143); título e menu responderam a Select.
    // Alturas 121..146 também selecionam a categoria lógica 2 (0x10584).
    Profile {
        module: "4e5e3d63181aca12e217a5b5685489b17f11fded49f7f02a58301384956436cd",
        mif: "c452553389de7cd3216d796e84a70b2382f6b49b03a3e82eb868c9ec735f2ac5",
        width: 120,
        height: 146,
    },
    // game-0382: canvas compatível conferido com o layout nativo do cache.
    Profile {
        module: "3786a3f4af6841f489a279e2575dc64282afb47cb1b3f39ad4d70d8cb17043a7",
        mif: "bda94290d1941ec2ea2d1f731568fd080ab700636515eb1e7f294e199a290a5f",
        width: 176,
        height: 220,
    },
    // game-0413: geometria compatível testada; menu interativo observado por 20 s.
    Profile {
        module: "afdc490585d32cde12fd620e31f89545a4958f79ba00430dfb61444395c83f83",
        mif: "59a33581373fbcc3b5c64cd3c03b4303cba5d8d51b02f6991280e1a1f5cfeb00",
        width: 128,
        height: 160,
    },
    // game-0436: geometria compatível testada; autenticação remota não confirmada.
    Profile {
        module: "de933b513846a29db9511a2df9e99d15ad457adcfb2cec0066aca5f3e0eb17a0",
        mif: "c16c3310bf6a74e0c4c5512d6a47454ca9a68249ad4f09033de516648095ca0d",
        width: 128,
        height: 160,
    },
    // game-0468.
    Profile {
        module: "e0e5a3d30bba2a891df685244c607b58f6e99ffe4939ff4aa75a6878db7c907c",
        mif: "42da103f2f0c2dbf5ddddfb1811a5b9a74195d1f95c5c4dbf4e66a833e8b9f5e",
        width: 120,
        height: 160,
    },
    // game-0484: três colunas de tiles cobrem o canvas lógico 120x160.
    Profile {
        module: "e6d87f2922e0efb1a7f0484fd707cb8be198c5ea47e276951abe2e08e3e64038",
        mif: "25a6c48c6c9dd5c1ba42838786cf81dd96df9eaa22e4d201fb88dfc596ccffaa",
        width: 120,
        height: 160,
    },
    // game-0485.
    Profile {
        module: "565e5be67e6de22762e4b6d162645b271779aa94dd2f45ce9ae6273bfb9237ef",
        mif: "76f43ffc8d3d79eaa1671a5782404d32641f3dc28d686912a34784f97da3dd99",
        width: 176,
        height: 220,
    },
    // game-0486.
    Profile {
        module: "a7c4d2c9c1f9cea0ff22586adad2e1e2fae8c9d30897a024b94a4aa0ad836011",
        mif: "ad41a659c1dd07907471dc871e3eb8dbc9e65155643a7189b7bc61486d7adbc8",
        width: 176,
        height: 220,
    },
    // game-0487.
    Profile {
        module: "a9446ba9479aa441f018ebb870c598a8043705c1ecbb0f5bfde8383691b2ccfe",
        mif: "a117daeaf5603ca32b3910830736d1c63064f82555e944464174b97310bf9a2a",
        width: 176,
        height: 220,
    },
    // game-0488.
    Profile {
        module: "d708401a8d5aa84c770aa6f3f9b36fa7c1216014986d069a18f23d3cf8058195",
        mif: "1bfced1138cc0d30b17c1e266f30221856c7e005351e4187263d0178edcb13d8",
        width: 176,
        height: 220,
    },
    // game-0534.
    Profile {
        module: "1ff783a1d12a9d25b6a78ecce5c9280d844762029037138e6504926c07144dc7",
        mif: "4627ac6361d5865f788dd4d367b0c322814494ced54a98a2ccc9fc68c6152a12",
        width: 176,
        height: 220,
    },
    // game-0539: título RGB565 120x115 no canvas compatível 120x130.
    Profile {
        module: "f693a0ff75f98efe034c88f36e0dd24ec6b7302190b7dbc34d6a4914e53a07e8",
        mif: "8149b5e92bf6a9c51730acaa271e698ea068778b35e98106d0069f4904c42e95",
        width: 120,
        height: 130,
    },
    // game-0586.
    Profile {
        module: "ff4f5dbecb90b214f3902892ed29effb93668861b62bc04fddcc07a0f29063d3",
        mif: "4ecf12e6f711873e86d7dfafbbaa779b9ef0a578e67d1a3e5c481313baf678c1",
        width: 800,
        height: 480,
    },
    // game-0670: canvas lógico 120x130; o título usa as primeiras 115 linhas.
    Profile {
        module: "9fe4954e114fcbeeb9c16452c0108ea8536b5a02b09b2c131c861113c4537f4d",
        mif: "dcd418e20007ce90254fb1b3f520d295aa8b18a163f5635cc48d586389f6e604",
        width: 120,
        height: 130,
    },
];

#[cfg(test)]
const SYNTHETIC: Profile = Profile {
    module: "aab3a239f8d23070ef69354142ab9f5d89a928c8d82a78e12abd0195f1530c06",
    mif: "654f7915e742041bd1c998121c401ce91bdc50c4e430651f85a96c4cfb702bf3",
    width: 176,
    height: 220,
};

fn profile(hash: &str) -> Option<&'static Profile> {
    #[cfg(test)]
    if hash == SYNTHETIC.module {
        return Some(&SYNTHETIC);
    }
    PROFILES.iter().find(|entry| entry.module == hash)
}

pub(super) fn geometry(hash: &str, module: &Path) -> Option<DisplayGeometry> {
    // Jogos desconhecidos não devem varrer MIFs a cada cálculo de Auto na biblioteca.
    let expected = profile(hash)?;
    let index = Index::for_root(module.parent()?);
    // A prioridade local/compartilhada e a rejeição de ambiguidade são as do launcher.
    // Uma cópia conhecida noutro lugar não pode substituir o MIF efetivamente escolhido.
    let selected = index.find(module).ok()??;
    let metadata = std::fs::symlink_metadata(&selected).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_MIF {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(selected)
        .ok()?
        .take(MAX_MIF + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_MIF || super::digest(&bytes) != expected.mif {
        return None;
    }
    if expected.module == "e5061cfbc69ee8eea080a9b15809092c74d5601d374a2df417384f6bf6440200" {
        let resource = module.with_file_name("bejeweled.rlb");
        let metadata = std::fs::symlink_metadata(&resource).ok()?;
        if !metadata.file_type().is_file() || metadata.len() != 362_034 { return None; }
        let mut bytes = Vec::new();
        std::fs::File::open(resource).ok()?.take(362_035).read_to_end(&mut bytes).ok()?;
        if bytes.len() != 362_034 || super::digest(&bytes)
            != "a6b549b40bf8882c1b1b854f74c8c7b51bb0bbc9be306d696d342a22f732170f" {
            return None;
        }
    }
    if expected.module == "9bc21120c00d32dc53f30bcc4fc09d381af60e6cc9e854d9ead7e45cd377ec0c" {
        let resource = module.with_file_name("mini2005.mbr");
        let metadata = std::fs::symlink_metadata(&resource).ok()?;
        if !metadata.file_type().is_file() || metadata.len() != 74_955 { return None; }
        let mut bytes = Vec::new();
        std::fs::File::open(resource).ok()?.take(74_956).read_to_end(&mut bytes).ok()?;
        if bytes.len() != 74_955 || super::digest(&bytes)
            != "8912ae24df1628cd37a5284fda7beb03cf09a0d31a6dc000ddac3ea08b4ca712" {
            return None;
        }
    }
    Some(DisplayGeometry {
        width: expected.width,
        height: expected.height,
        depth: 16,
    })
}
