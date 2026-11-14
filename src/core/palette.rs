//! 绘图配色：31 套内置方案 + 符号专用色板。
//!
//! 配色分两个来源，**顺序稳定**（工程文件按枚举名序列化，旧工程仍可读）：
//! - **原有 7 套**（`Aura` 起）：品牌与期刊常用，保留原名原色不变；
//! - **析光 24 套**（`Tableau10` 起）：移植自 spectra 的 26 套内置方案，
//!   去掉与原有重复的 Okabe-Ito / Tol 两套。色数不固定：多数 6 色，
//!   暖橙—冷蓝 / 莫兰迪 / 红橙青绿 / 橙绿蓝紫 为 9 色，部分为 8 色。
//!
//! 移植时的两条硬规则（与 spectra 一致）：
//! 1. **色值逐字节照抄**，不做任何「微调」——同一张图在两个软件里必须同色；
//! 2. 色数不固定，取色一律 `index % len` 回绕，不假设 8 色。
//!
//! 另附 13 色**符号专用色板**（散点符号、误差棒等）：与 26 套主体方案
//! 重叠度低，白底对比度足够，便于区分「柱体所属数据集」与「符号本身」。

use serde::{Deserialize, Serialize};

use crate::core::color::Rgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PaletteId {
    // ── 原有 7 套（名称与色值保持不变，保证旧工程可读）──
    Aura,
    OkabeIto,
    TolMuted,
    Npg,
    Lancet,
    Jco,
    Grayscale,
    // ── 移植自 spectra 的 24 套（顺序与 spectra COLORMAPS 对应）──
    Tableau10,
    D3,
    Ggplot2,
    Set1,
    Set2,
    Pastel1,
    Pastel2,
    Dark2,
    Carto,
    Viridis,
    Plasma,
    Inferno,
    Magma,
    Nature,
    Science,
    Jama,
    WarmOrangeCoolBlue,
    Morandi,
    CoralWarm,
    RedOrangeTurquoise,
    OrangeGreenBluePurple,
    BlueRedGradient,
    IndigoProgression,
    SteelBlueProgression,
}

impl PaletteId {
    pub const ALL: [PaletteId; 31] = [
        Self::Aura,
        Self::OkabeIto,
        Self::TolMuted,
        Self::Npg,
        Self::Lancet,
        Self::Jco,
        Self::Grayscale,
        Self::Tableau10,
        Self::D3,
        Self::Ggplot2,
        Self::Set1,
        Self::Set2,
        Self::Pastel1,
        Self::Pastel2,
        Self::Dark2,
        Self::Carto,
        Self::Viridis,
        Self::Plasma,
        Self::Inferno,
        Self::Magma,
        Self::Nature,
        Self::Science,
        Self::Jama,
        Self::WarmOrangeCoolBlue,
        Self::Morandi,
        Self::CoralWarm,
        Self::RedOrangeTurquoise,
        Self::OrangeGreenBluePurple,
        Self::BlueRedGradient,
        Self::IndigoProgression,
        Self::SteelBlueProgression,
    ];

    /// 分类：下拉框按组显示，避免 31 项平铺成一长条。
    pub fn group(self) -> PaletteGroup {
        use PaletteGroup::*;
        match self {
            Self::Aura => Brand,
            Self::OkabeIto | Self::TolMuted => Colorblind,
            Self::Npg
            | Self::Lancet
            | Self::Jco
            | Self::Nature
            | Self::Science
            | Self::Jama
            | Self::Grayscale => Journal,
            Self::Tableau10
            | Self::D3
            | Self::Ggplot2
            | Self::Set1
            | Self::Set2
            | Self::Pastel1
            | Self::Pastel2
            | Self::Dark2
            | Self::Carto
            | Self::WarmOrangeCoolBlue
            | Self::Morandi
            | Self::CoralWarm
            | Self::RedOrangeTurquoise
            | Self::OrangeGreenBluePurple => Categorical,
            Self::Viridis
            | Self::Plasma
            | Self::Inferno
            | Self::Magma
            | Self::BlueRedGradient
            | Self::IndigoProgression
            | Self::SteelBlueProgression => Gradient,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Aura => "Aura（现代）",
            Self::OkabeIto => "Okabe–Ito（色盲友好）",
            Self::TolMuted => "Tol Muted（柔和）",
            Self::Npg => "NPG（Nature）",
            Self::Lancet => "Lancet（柳叶刀）",
            Self::Jco => "JCO（肿瘤学）",
            Self::Grayscale => "灰度（印刷）",
            Self::Tableau10 => "Tableau 10",
            Self::D3 => "D3 分类",
            Self::Ggplot2 => "ggplot2 默认",
            Self::Set1 => "Set1 深色",
            Self::Set2 => "Set2 柔和",
            Self::Pastel1 => "Pastel1 淡彩",
            Self::Pastel2 => "Pastel2 淡彩",
            Self::Dark2 => "Dark2 深彩",
            Self::Carto => "Carto 地图",
            Self::Viridis => "Viridis 渐变",
            Self::Plasma => "Plasma 渐变",
            Self::Inferno => "Inferno 渐变",
            Self::Magma => "Magma 渐变",
            Self::Nature => "Nature 期刊",
            Self::Science => "Science 期刊",
            Self::Jama => "JAMA 期刊",
            Self::WarmOrangeCoolBlue => "暖橙—冷蓝 9 色",
            Self::Morandi => "莫兰迪自然 9 色",
            Self::CoralWarm => "珊瑚暖橙 8 色",
            Self::RedOrangeTurquoise => "红橙青绿 9 色",
            Self::OrangeGreenBluePurple => "橙绿蓝紫 9 色",
            Self::BlueRedGradient => "蓝红渐变 8 级",
            Self::IndigoProgression => "靛蓝分级 6 级",
            Self::SteelBlueProgression => "钢蓝分级 6 级",
        }
    }

    /// 色值表。**逐字节照抄来源**，色数不固定（6 / 8 / 9）。
    pub fn colors(self) -> Vec<Rgba> {
        let hexes: &[&str] = match self {
            Self::Aura => &[
                "#5B6CFF", "#17B8A6", "#FF7A59", "#B07CFF", "#F2B84B", "#3D4A5C", "#E85D9E",
                "#4C9F70",
            ],
            Self::OkabeIto => &[
                "#0072B2", "#D55E00", "#009E73", "#CC79A7", "#E69F00", "#56B4E9", "#F0E442",
                "#000000",
            ],
            Self::TolMuted => &[
                "#332288", "#88CCEE", "#44AA99", "#117733", "#999933", "#DDCC77", "#CC6677",
                "#882255",
            ],
            Self::Npg => &[
                "#E64B35", "#4DBBD5", "#00A087", "#3C5488", "#F39B7F", "#8491B4", "#91D1C2",
                "#DC0000",
            ],
            Self::Lancet => &[
                "#00468B", "#ED0000", "#42B540", "#0099B4", "#925E9F", "#FDAF91", "#AD002A",
                "#ADB6B6",
            ],
            Self::Jco => &[
                "#0073C2", "#EFC000", "#868686", "#CD534C", "#7AA6DC", "#003C67", "#8F7700",
                "#A73030",
            ],
            Self::Grayscale => &[
                "#1A1A1A", "#4D4D4D", "#737373", "#999999", "#B3B3B3", "#CCCCCC", "#E0E0E0",
                "#F2F2F2",
            ],
            // ── 以下移植自 spectra，色值未做任何改动 ──
            Self::Tableau10 => &[
                "#1F77B4", "#FF7F0E", "#2CA02C", "#D62728", "#9467BD", "#8C564B",
            ],
            Self::D3 => &[
                "#1C789F", "#FF7F0E", "#2CA02C", "#D62728", "#9467BD", "#8C564B",
            ],
            Self::Ggplot2 => &[
                "#F8766D", "#7CAE00", "#00BFC4", "#C77CFF", "#FFBB00", "#929292",
            ],
            Self::Set1 => &[
                "#E41A1C", "#377EB8", "#4DAF4A", "#984EA3", "#FF7F00", "#A65628",
            ],
            Self::Set2 => &[
                "#66C2A5", "#FC8D62", "#8DA0CB", "#E78AC3", "#A6D854", "#FFD92F",
            ],
            Self::Pastel1 => &[
                "#FBB4AE", "#B3CDE3", "#CCEBC5", "#DECBE4", "#FED9A6", "#FFFFCC",
            ],
            Self::Pastel2 => &[
                "#B3E2CD", "#FDCDAC", "#CBD5E8", "#F4CAE4", "#E6F5C9", "#FFF2AE",
            ],
            Self::Dark2 => &[
                "#1B9E77", "#D95F02", "#7570B3", "#E7298A", "#66A61E", "#E6AB02",
            ],
            Self::Carto => &[
                "#7EB0D5", "#F06E5A", "#8CC882", "#E6AF5A", "#BE96C8", "#A0A0A0",
            ],
            Self::Viridis => &[
                "#440154", "#3B528B", "#21918C", "#5EC962", "#C8E23C", "#FDE725",
            ],
            Self::Plasma => &[
                "#0D0887", "#7E03A8", "#CC4778", "#F89540", "#FEC72C", "#F0F921",
            ],
            Self::Inferno => &[
                "#000004", "#57106E", "#BC3754", "#F98E09", "#FCC83C", "#FCFFA4",
            ],
            Self::Magma => &[
                "#000004", "#50127B", "#B63679", "#FA884E", "#FDC95F", "#FCFDBF",
            ],
            Self::Nature => &[
                "#E31A1C", "#1F78B4", "#33A02C", "#FF7F00", "#9467BD", "#000000",
            ],
            Self::Science => &[
                "#3E609E", "#BE4C33", "#60913C", "#96609E", "#D69733", "#787878",
            ],
            Self::Jama => &[
                "#374C80", "#C44639", "#3F8D9E", "#889E52", "#D5A04A", "#787878",
            ],
            Self::WarmOrangeCoolBlue => &[
                "#E64532", "#F5A65B", "#EBF0B5", "#376439", "#A8DDE1", "#2C4CA0", "#B53289",
                "#EBCCE2", "#CFEADF",
            ],
            Self::Morandi => &[
                "#C89A9C", "#E7C5C4", "#C5AC45", "#DCD2A1", "#CCE4C2", "#7FAB87", "#8CB8D3",
                "#BBD4BF", "#E9E5E6",
            ],
            Self::CoralWarm => &[
                "#ED5F5F", "#FCA3A3", "#FFA74F", "#FDCD8C", "#53A8E1", "#AFD3E7", "#9A72C7",
                "#D0BBDB",
            ],
            Self::RedOrangeTurquoise => &[
                "#E73A36", "#C79494", "#F39D49", "#8EC321", "#3D7D35", "#B3CCB0", "#AFE5F6",
                "#30BBEA", "#E8D5D5",
            ],
            Self::OrangeGreenBluePurple => &[
                "#F27830", "#BDBF32", "#E8E7BB", "#C6DFB8", "#24A645", "#1D73B6", "#8768A6",
                "#D7CAE4", "#D5EBF0",
            ],
            Self::BlueRedGradient => &[
                "#08306B", "#2170B5", "#6BAED6", "#C6DBEF", "#FEE0D2", "#FC9271", "#EF3B2C",
                "#A50F15",
            ],
            Self::IndigoProgression => &[
                "#424D95", "#6182CC", "#9DABD0", "#90B8F1", "#B9D8F7", "#C0DAF0",
            ],
            Self::SteelBlueProgression => &[
                "#E8EAF6", "#BBCFE8", "#92BBD7", "#659BCA", "#376EAE", "#234086",
            ],
        };
        hexes.iter().map(|h| Rgba::hex(h)).collect()
    }

    pub fn color(self, index: usize) -> Rgba {
        let colors = self.colors();
        colors[index % colors.len()]
    }

    pub fn is_colorblind_safe(self) -> bool {
        matches!(self, Self::OkabeIto | Self::TolMuted)
    }

    /// 色数（下拉框预览按实际色数铺色块，不假设 8 个）。
    pub fn len(self) -> usize {
        self.colors().len()
    }

    pub fn is_empty(self) -> bool {
        false
    }
}

/// 配色分类，用于下拉框分组显示。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteGroup {
    Brand,
    Colorblind,
    Journal,
    Categorical,
    Gradient,
}

impl PaletteGroup {
    pub const ALL: [PaletteGroup; 5] = [
        Self::Brand,
        Self::Colorblind,
        Self::Journal,
        Self::Categorical,
        Self::Gradient,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Brand => "品牌 / 现代",
            Self::Colorblind => "色盲友好",
            Self::Journal => "期刊 / 投稿",
            Self::Categorical => "分类 / 多色相",
            Self::Gradient => "渐变 / 分级",
        }
    }

    /// 该组下的全部配色，保持 `PaletteId::ALL` 的相对顺序。
    pub fn palettes(self) -> Vec<PaletteId> {
        PaletteId::ALL
            .iter()
            .copied()
            .filter(|p| p.group() == self)
            .collect()
    }
}

/// 符号专用色板（13 色）：散点符号、误差棒、图例标记等单独取色。
///
/// 选色原则（与 spectra 一致）：与 26 套主体方案重叠度低，
/// 白底上对比度足够，便于把「柱体属于哪个数据集」和「符号本身」分开看。
pub const SYMBOL_PALETTE: [(&str, &str); 13] = [
    ("Black", "#1A1A1A"),
    ("Dark red", "#B02A20"),
    ("Vermillion", "#D55E00"),
    ("Orange", "#E69F00"),
    ("Yellow", "#F0E442"),
    ("Green", "#009E73"),
    ("Teal", "#009192"),
    ("Blue", "#0072B2"),
    ("Sky blue", "#56B4E9"),
    ("Purple", "#7257A8"),
    ("Magenta", "#CC79A7"),
    ("Brown", "#8C564B"),
    ("Gray", "#8C8C8C"),
];

/// 取符号专用色（索引自动回绕）。
pub fn symbol_color(index: usize) -> Rgba {
    let (_, hex) = SYMBOL_PALETTE[index % SYMBOL_PALETTE.len()];
    Rgba::hex(hex)
}

/// 符号专用色的中文名。
pub fn symbol_color_label(index: usize) -> &'static str {
    let (en, _) = SYMBOL_PALETTE[index % SYMBOL_PALETTE.len()];
    match en {
        "Black" => "黑色",
        "Dark red" => "暗红",
        "Vermillion" => "朱红",
        "Orange" => "橙",
        "Yellow" => "黄",
        "Green" => "绿",
        "Teal" => "青",
        "Blue" => "蓝",
        "Sky blue" => "天蓝",
        "Purple" => "紫",
        "Magenta" => "品红",
        "Brown" => "棕",
        _ => "灰",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 三十一套配色齐全且色数不少于6() {
        assert_eq!(PaletteId::ALL.len(), 31);
        for p in PaletteId::ALL {
            let c = p.colors();
            assert!(c.len() >= 6, "{} 只有 {} 色", p.label(), c.len());
            assert!(c.iter().all(|c| c.a == 255), "{} 有透明色", p.label());
            assert!(!p.label().is_empty());
        }
    }

    #[test]
    fn 无重复枚举项() {
        let mut seen: Vec<PaletteId> = Vec::new();
        for p in PaletteId::ALL {
            assert!(!seen.contains(&p), "PaletteId::ALL 有重复项 {:?}", p);
            seen.push(p);
        }
    }

    #[test]
    fn 色值与spectra逐字节一致() {
        // 移植验收：抽几套关键色比对原始字节，确保没有「顺手微调」。
        let tableau = PaletteId::Tableau10.colors();
        assert_eq!(
            (tableau[0].r, tableau[0].g, tableau[0].b),
            (0x1F, 0x77, 0xB4)
        );
        assert_eq!(
            (tableau[1].r, tableau[1].g, tableau[1].b),
            (0xFF, 0x7F, 0x0E)
        );
        let viridis = PaletteId::Viridis.colors();
        assert_eq!(
            (viridis[0].r, viridis[0].g, viridis[0].b),
            (0x44, 0x01, 0x54)
        );
        assert_eq!(
            (viridis[5].r, viridis[5].g, viridis[5].b),
            (0xFD, 0xE7, 0x25)
        );
        let morandi = PaletteId::Morandi.colors();
        assert_eq!(morandi.len(), 9);
        assert_eq!(
            (morandi[0].r, morandi[0].g, morandi[0].b),
            (0xC8, 0x9A, 0x9C)
        );
    }

    #[test]
    fn 分组覆盖全部配色且互不重叠() {
        let all: Vec<PaletteId> = PaletteGroup::ALL
            .iter()
            .flat_map(|g| g.palettes())
            .collect();
        assert_eq!(all.len(), PaletteId::ALL.len(), "分组没覆盖全或有重复");
    }

    #[test]
    fn 色数不固定时取色回绕() {
        // 暖橙—冷蓝是 9 色，取到第 20 个应当回绕到第 2 个
        let p = PaletteId::WarmOrangeCoolBlue;
        assert_eq!(p.len(), 9);
        assert_eq!(p.color(20), p.color(2));
        // 灰度是 8 色
        assert_eq!(PaletteId::Grayscale.len(), 8);
    }

    #[test]
    fn 色盲友好标记正确() {
        assert!(PaletteId::OkabeIto.is_colorblind_safe());
        assert!(PaletteId::TolMuted.is_colorblind_safe());
        assert!(!PaletteId::Tableau10.is_colorblind_safe());
        assert!(!PaletteId::Aura.is_colorblind_safe());
    }

    #[test]
    fn 符号色板十三色且可回绕() {
        assert_eq!(SYMBOL_PALETTE.len(), 13);
        for i in 0..13 {
            let c = symbol_color(i);
            assert!(c.a == 255);
            assert!(!symbol_color_label(i).is_empty());
        }
        assert_eq!(symbol_color(13), symbol_color(0));
    }

    #[test]
    fn 每套配色内部无重复色() {
        // 分类 / 渐变方案若出现重复色，图上会认不出哪个数据集是哪个。
        for p in PaletteId::ALL {
            let c = p.colors();
            for i in 0..c.len() {
                for j in (i + 1)..c.len() {
                    assert!(c[i] != c[j], "{} 的第 {} 与第 {} 色重复", p.label(), i, j);
                }
            }
        }
    }
}
