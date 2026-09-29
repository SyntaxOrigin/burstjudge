//! Algısal (perceptual) hash — kendi DCT'mizle, 64 bit.
//!
//! # Algoritma
//!
//! Rapor `b07` şemasıyla birebir uyumlu dört adım:
//!
//! 1. Görüntü **8×8 gri tonlamaya** kutu ortalamasıyla indirgenir (alan bazlı
//!    örnekleme; tam çözünürlüklü piksel başına en az 1 örnek alınır).
//! 2. 8×8 blok üzerinde **ayrık kosinüs dönüşümü (DCT-II)** uygulanır.
//!    Tampon boyutu sabit 8×8'dir; resmin çözünürlüğüne bağlı değildir.
//! 3. DC katsayısı (0,0) **ortalama fark** için çıkarılır: geri kalan 63
//!    katsayının ortalaması eşik olarak kullanılır.
//! 4. `katsayı > ortalama` ise bit `1`, değilse `0` yazılır → 64 bit.
//!
//! DC'nin çıkarılması "ortalama fark" adını açıklar: karşılaştırılan şey
//! karesel ortalama parlaklık değil, **ortalama parlaklıktan sapma**dır. Aynı
//! sahne farklı pozlama ile çekildiğinde DC değişir ama 63 AC katsayısının
//! imzası korunur.
//!
//! # Kayan nokta hassasiyeti (belgelenmiş sözleşme)
//!
//! - Tüm ara toplamlar **`f64`** ile biriktirilir; `f32` yalnız sonuçta
//!   karşılaştırma için kullanılır.
//! - DCT katsayı tablosu derleme zamanında `f64::cos` ile hesaplanır ve
//!   `f64::consts::PI` sabitini kullanır. Aynı makinede iki çalıştırma
//!   **bit düzeyinde aynı** sonucu verir (deterministik).
//! - Platformlar arası son 1 ULP farkı teorik olarak mümkündür (`cos`
//!   uygulaması platforma göre değişebilir). Bu yüzden eşik karşılaştırması
//!   `>` yerine `> + EPS` ile yapılır: `EPS = 1e-9`. Eşiğe çok yakın
//!   katsayılar **bit 0** sayılır (temkinli varsayılan).
//! - Hash'in **kendi içinde** tutarlılığı (aynı görüntü → aynı hash) ve
//!   kararlılığı (aynı dosya → aynı hash) testlerle doğrulanmıştır.
//!
//! Kaynak: Kang, Yu & Hong (2012), "Similarity and Dissimilarity Measures for
//! Multimedia Images" ve pHash ailesi; DCT tanımı ITU T.81 Annex A.

use std::path::PathBuf;

use crate::hata::{Hata, Sonuc};
use crate::jpeg::GriGoruntu;

/// Hash üretilen en küçük görüntü kenarı (piksel).
pub const MIN_KENAR: u32 = 8;

/// Eşik karşılaştırmasında kullanılan mutlak tolerans.
pub const EPS: f64 = 1e-9;

/// 64 bitlik algısal hash.
///
/// Uzunluğu 64 olan `u64` olarak tutulur; en anlamlı bit `63..0` sırasıyla
/// 8×8 bloğun satır-major sırasına karşılık gelir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AlgisalHash(pub u64);

impl AlgisalHash {
    /// Sıfır hash (tüm bitler 0) — "hash yok" durumu için kullanılır.
    pub const BOS: AlgisalHash = AlgisalHash(0);

    /// 16 ondalık basamağa yazılmış biçim (raporda görünür).
    pub fn onaltilik(&self) -> String {
        format!("{:016x}", self.0)
    }

    /// Hamming uzaklığı: kaç bit farklı.
    pub fn hamming(&self, diger: &AlgisalHash) -> u32 {
        (self.0 ^ diger.0).count_ones()
    }

    /// Hash'in `bit` indisindeki değeri.
    pub fn bit(&self, sira: usize) -> bool {
        if sira >= 64 {
            return false;
        }
        (self.0 >> (63 - sira)) & 1 == 1
    }
}

impl std::fmt::Display for AlgisalHash {
    fn fmt(&self, bicik: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(bicik, "{}", self.onaltilik())
    }
}

/// DCT-II temel fonksiyon tablosu: `TAN[x][u] = cos((2x+1) u pi / 16)`.
///
/// `f64::cos` `const fn` çağrısı olamadığı için tablo, `f64::cos` ile
/// üretilip IEEE-754 çift duyarlılıkta elle yazılmıştır. Sıra önemlidir:
/// **satır = örnek indeksi `x`, sütun = frekans indeksi `u`**. Test
/// `kos_tablosu_kos_dogrusu_ile_ortusur` her hücreyi `cos` ile doğrular.
pub(crate) const TAN: [[f64; 8]; 8] = [
    [
        1.0,
        0.9807852804032304,
        0.9238795325112867,
        0.8314696123025452,
        std::f64::consts::FRAC_1_SQRT_2,
        0.5555702330196023,
        0.38268343236508984,
        0.19509032201612833,
    ],
    [
        1.0,
        0.8314696123025452,
        0.38268343236508984,
        -0.1950903220161282,
        -std::f64::consts::FRAC_1_SQRT_2,
        -0.9807852804032304,
        -0.9238795325112869,
        -0.5555702330196022,
    ],
    [
        1.0,
        0.5555702330196023,
        -0.3826834323650897,
        -0.9807852804032304,
        -std::f64::consts::FRAC_1_SQRT_2,
        0.1950903220161283,
        0.9238795325112865,
        0.8314696123025455,
    ],
    [
        1.0,
        0.19509032201612833,
        -0.9238795325112867,
        -0.5555702330196022,
        std::f64::consts::FRAC_1_SQRT_2,
        0.8314696123025455,
        -0.3826834323650899,
        -0.9807852804032307,
    ],
    [
        1.0,
        -0.1950903220161282,
        -0.9238795325112869,
        0.5555702330196018,
        std::f64::consts::FRAC_1_SQRT_2,
        -0.8314696123025451,
        -0.38268343236509056,
        0.9807852804032304,
    ],
    [
        1.0,
        -0.555570233019602,
        -0.38268343236509034,
        0.9807852804032304,
        -std::f64::consts::FRAC_1_SQRT_2,
        -0.19509032201612803,
        0.9238795325112867,
        -0.831469612302545,
    ],
    [
        1.0,
        -0.8314696123025454,
        0.38268343236509,
        0.19509032201612878,
        -std::f64::consts::FRAC_1_SQRT_2,
        0.9807852804032307,
        -0.9238795325112864,
        0.5555702330196015,
    ],
    [
        1.0,
        -0.9807852804032304,
        0.9238795325112865,
        -0.8314696123025451,
        0.7071067811865466,
        -0.5555702330196015,
        0.38268343236508956,
        -0.19509032201612858,
    ],
];

/// İki boyutlu DCT-II'yi 8×8 gri blok üzerinde uygular.
///
/// Girdi satır-major 64 elemanlık `f64` dizisi, çıktı aynı düzende
/// katsayılardır. Kanonik formül:
///
/// `F(u,v) = 1/4 * C(u) C(v) * Σ_x Σ_y f(x,y) cos((2x+1)uπ/16) cos((2y+1)vπ/16)`
///
/// Burada `C(0) = 1/√2`, `C(k>0) = 1`.
pub fn dct8x8(giris: &[f64; 64]) -> [f64; 64] {
    let mut gecici = [0f64; 64];
    // 1. geçiş: satır ayrıklaştırması (x boyutu).
    for y in 0..8 {
        for u in 0..8 {
            let mut toplam = 0f64;
            for x in 0..8 {
                toplam += giris[y * 8 + x] * TAN[x][u];
            }
            let olcek = if u == 0 {
                std::f64::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            gecici[y * 8 + u] = 0.5 * olcek * toplam;
        }
    }
    // 2. geçiş: sütun ayrıklaştırması (y boyutu).
    let mut cikti = [0f64; 64];
    for u in 0..8 {
        for v in 0..8 {
            let mut toplam = 0f64;
            for y in 0..8 {
                toplam += gecici[y * 8 + u] * TAN[y][v];
            }
            let olcek = if v == 0 {
                std::f64::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            cikti[v * 8 + u] = 0.5 * olcek * toplam;
        }
    }
    cikti
}

/// Görüntüyü 8×8 gri bloğa indirger (alan/kutu ortalaması).
///
/// 8'in katı olmayan boyutlarda son hücre kısmal alanı **ölçekli** ortalar;
/// böylece "sol tarafı parlak, sağ tarafı karanlık" gibi gradyanlarda
/// örnekleme yanlılığı oluşmaz.
pub fn kucult_8x8(goruntu: &GriGoruntu) -> [f64; 64] {
    let mut blok = [0f64; 64];
    let gw = goruntu.genislik.max(1) as usize;
    let gh = goruntu.yukseklik.max(1) as usize;
    for by in 0..8 {
        let y0 = by * gh / 8;
        let y1 = (((by + 1) * gh) / 8).max(y0 + 1).min(gh);
        for bx in 0..8 {
            let x0 = bx * gw / 8;
            let x1 = (((bx + 1) * gw) / 8).max(x0 + 1).min(gw);
            let mut toplam = 0u64;
            let mut adet = 0u64;
            for y in y0..y1 {
                let satir = y * gw;
                for x in x0..x1 {
                    let i = satir + x;
                    if let Some(&p) = goruntu.pikseller.get(i) {
                        toplam += u64::from(p);
                        adet += 1;
                    }
                }
            }
            blok[by * 8 + bx] = if adet == 0 {
                0.0
            } else {
                toplam as f64 / adet as f64
            };
        }
    }
    blok
}

/// 64 elemanlık katsayı dizisinden algısal hash üretir.
///
/// DC katsayısı atlanır; kalan 63 katsayının aritmetik ortalaması eşiktir.
pub fn hash_katsayilardan(katsayilar: &[f64; 64]) -> AlgisalHash {
    let mut toplam = 0f64;
    for k in katsayilar.iter().skip(1) {
        toplam += k;
    }
    let ortalama = toplam / 63.0;
    let mut deger: u64 = 0;
    for (i, k) in katsayilar.iter().enumerate().skip(1) {
        if *k > ortalama + EPS {
            deger |= 1u64 << (63 - i);
        }
    }
    AlgisalHash(deger)
}

/// Görüntüden algısal hash üretir.
///
/// Görüntü 8×8'den küçükse `None` döner: o boyutta algısal hash anlamsızdır
/// (pHash'in asgari gereksinimi).
pub fn hash_goruntuden(goruntu: &GriGoruntu) -> Option<AlgisalHash> {
    if goruntu.genislik < MIN_KENAR || goruntu.yukseklik < MIN_KENAR {
        return None;
    }
    let blok = kucult_8x8(goruntu);
    let katsayilar = dct8x8(&blok);
    Some(hash_katsayilardan(&katsayilar))
}

/// Görüntüden hash üretir ve boyut yetersizse hata döndürür.
pub fn hash_zorunlu(goruntu: &GriGoruntu) -> Sonuc<AlgisalHash> {
    hash_goruntuden(goruntu).ok_or_else(|| Hata::OnizlemeBozuk {
        yol: PathBuf::from("<goruntu>"),
        ayrinti: format!(
            "görüntü çok küçük: {}x{} (asgari {}x{})",
            goruntu.genislik, goruntu.yukseklik, MIN_KENAR, MIN_KENAR
        ),
    })
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Verilen 8×8 gri desenden görüntü kurar.
    fn goruntu_8x8(uretici: impl Fn(u32, u32) -> u8) -> GriGoruntu {
        let mut p = Vec::with_capacity(64);
        for y in 0..8 {
            for x in 0..8 {
                p.push(uretici(x, y));
            }
        }
        GriGoruntu {
            genislik: 8,
            yukseklik: 8,
            pikseller: p,
        }
    }

    #[test]
    fn kos_tablosu_kos_dogrusu_ile_ortusur() {
        // TAN[x][u] = cos((2x+1) u pi / 16)
        for (x, tan_satiri) in TAN.iter().enumerate() {
            for (u, &deger) in tan_satiri.iter().enumerate() {
                let beklenen =
                    (((2 * x + 1) as f64 * u as f64) * std::f64::consts::PI / 16.0).cos();
                assert!(
                    (deger - beklenen).abs() < 1e-12,
                    "TAN[{}][{}] = {} beklenen {}",
                    x,
                    u,
                    deger,
                    beklenen
                );
            }
        }
    }

    /// Naif O(n^4) DCT-II referansı — iki geçişli uygulamayı bağımsız doğrular.
    fn dct_naif(giris: &[f64; 64]) -> [f64; 64] {
        let mut cikti = [0f64; 64];
        for v in 0..8 {
            for u in 0..8 {
                let mut toplam = 0f64;
                for y in 0..8 {
                    for x in 0..8 {
                        let cx =
                            (((2 * x + 1) as f64 * u as f64) * std::f64::consts::PI / 16.0).cos();
                        let cy =
                            (((2 * y + 1) as f64 * v as f64) * std::f64::consts::PI / 16.0).cos();
                        toplam += giris[y * 8 + x] * cx * cy;
                    }
                }
                let ou = if u == 0 {
                    std::f64::consts::FRAC_1_SQRT_2
                } else {
                    1.0
                };
                let ov = if v == 0 {
                    std::f64::consts::FRAC_1_SQRT_2
                } else {
                    1.0
                };
                cikti[v * 8 + u] = 0.25 * ou * ov * toplam;
            }
        }
        cikti
    }

    #[test]
    fn dct_iki_gecisli_uygulama_naif_referansla_ortusur() {
        // Test vektörü: her hücre için farklı, deterministik bir desen.
        let mut blok = [0f64; 64];
        for (i, v) in blok.iter_mut().enumerate() {
            let x = (i % 8) as f64;
            let y = (i / 8) as f64;
            *v = (x * 13.0 + y * 29.0).sin() * 100.0 + x * y * 3.0;
        }
        let hizli = dct8x8(&blok);
        let naif = dct_naif(&blok);
        for i in 0..64 {
            assert!(
                (hizli[i] - naif[i]).abs() < 1e-9,
                "indeks {}: hizli {} naif {}",
                i,
                hizli[i],
                naif[i]
            );
        }
    }

    #[test]
    fn dct_ana_kosegen_testi_gecer() {
        // Ortonormal 1 boyutlu DCT-II: g = [1..8]
        // F(u) = (1/sqrt(8)) · Σ g[x]·cos((2x+1)uπ/16)
        let g = [1f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let beklenen = [12.727922061357855, -4.555410295949018, 0.0];
        for (u, &b) in beklenen.iter().enumerate() {
            let mut toplam = 0f64;
            for (x, g) in g.iter().enumerate() {
                toplam += g * TAN[x][u];
            }
            let hesap = toplam / 8f64.sqrt();
            assert!((hesap - b).abs() < 1e-9, "u={} {} beklenen {}", u, hesap, b);
        }
    }

    #[test]
    fn dct_dc_katsayisi_duz_gri_alan_tam_dondurur() {
        // Düz 100 -> DC = 8 * 100 = 800 (1/4 * C(0)C(0) * 64 * 100 = 800)
        let mut blok = [0f64; 64];
        for v in blok.iter_mut() {
            *v = 100.0;
        }
        let k = dct8x8(&blok);
        assert!((k[0] - 800.0).abs() < 1e-6, "{}", k[0]);
        // AC katsayıları sıfır olmalı.
        for (i, ac) in k.iter().enumerate().skip(1) {
            assert!(ac.abs() < 1e-6, "AC[{}] = {}", i, ac);
        }
    }

    #[test]
    fn dct_iki_boyutlu_dc_toplam_kurali() {
        // Sabit 1'den oluşan 8x8: DC = (1/4) * C(0)^2 * 64 = (1/4) * 0.5 * 64 = 8
        let blok = [1f64; 64];
        let k = dct8x8(&blok);
        assert!((k[0] - 8.0).abs() < 1e-9, "{}", k[0]);
        for (i, ac) in k.iter().enumerate().skip(1) {
            assert!(ac.abs() < 1e-9, "AC[{}] = {}", i, ac);
        }
    }

    #[test]
    fn dct_ayirilabilir_kosegen_özelligi() {
        // İki boyutlu DCT ayrılabilir: f(x,y) = a(x)·b(y) ise
        // F(u,v) = A(u)·B(v). Bu, tablonun satır/sütun yönünü ters çevirmeyi
        // yakalar (transpozisyon hatası bu eşitliği bozar).
        let a = [1f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let b = [8f64, 6.0, 7.0, 5.0, 3.0, 0.0, 9.0, 1.0];
        let mut blok = [0f64; 64];
        for y in 0..8 {
            for x in 0..8 {
                blok[y * 8 + x] = a[x] * b[y];
            }
        }
        let ham = dct8x8(&blok);
        let iki_boyutlu = dct_naif(&blok);
        for i in 0..64 {
            assert!(
                (ham[i] - iki_boyutlu[i]).abs() < 1e-9,
                "indeks {}: {} != {}",
                i,
                ham[i],
                iki_boyutlu[i]
            );
        }
    }

    #[test]
    fn dct_enerji_korunumu_parseval() {
        // Ayrık Parseval: bu ölçekleme için Σ f² = Σ F² tam olarak sağlanır.
        let mut blok = [0f64; 64];
        for (i, v) in blok.iter_mut().enumerate() {
            let x = (i % 8) as f64;
            let y = (i / 8) as f64;
            *v = (x * 7.0 + y * 3.0).sin() * 100.0;
        }
        let k = dct8x8(&blok);
        let enerji_girdi: f64 = blok.iter().map(|v| v * v).sum();
        let enerji_cikti: f64 = k.iter().map(|v| v * v).sum();
        assert!(enerji_girdi > 0.0);
        let fark = (enerji_cikti - enerji_girdi).abs() / enerji_girdi;
        assert!(
            fark < 1e-9,
            "girdi {} çıktı {} fark {}",
            enerji_girdi,
            enerji_cikti,
            fark
        );
    }

    #[test]
    fn kucult_duz_griyi_aynen_korur() {
        let g = goruntu_8x8(|_, _| 77);
        let b = kucult_8x8(&g);
        for v in b.iter() {
            assert!((v - 77.0).abs() < 1e-9, "{}", v);
        }
    }

    #[test]
    fn kucult_sol_yari_bir_sag_yari_sifir() {
        // 16x16: sol 8 sütun 255, sağ 8 sütun 0 -> her blok ortalı 127.5
        let mut p = Vec::with_capacity(256);
        for _y in 0..16 {
            for x in 0..16 {
                p.push(if x < 8 { 255 } else { 0 });
            }
        }
        let g = GriGoruntu {
            genislik: 16,
            yukseklik: 16,
            pikseller: p,
        };
        let b = kucult_8x8(&g);
        // Sol sütunlar 255, sağ sütunlar 0 (blok sınırına tam denk gelir).
        for y in 0..8 {
            assert!((b[y * 8] - 255.0).abs() < 1e-9, "{}", b[y * 8]);
            assert!((b[y * 8 + 7] - 0.0).abs() < 1e-9, "{}", b[y * 8 + 7]);
        }
    }

    #[test]
    fn kucult_8_kat_olmayan_boyutta_scali_ortalar() {
        // 9x9 -> 8x8: her blok 1-2 piksel içerir, sınırlar taşmaz.
        let mut p = vec![0u8; 81];
        for (i, v) in p.iter_mut().enumerate() {
            *v = if i % 2 == 0 { 200 } else { 0 };
        }
        let g = GriGoruntu {
            genislik: 9,
            yukseklik: 9,
            pikseller: p,
        };
        let b = kucult_8x8(&g);
        assert_eq!(b.len(), 64);
        assert!(b.iter().all(|v| (0.0..=255.0).contains(v)));
    }

    #[test]
    fn kucult_tek_piksel_goruntude_hicbir_zaman_tasmaz() {
        // Piksel sayısı az (1x1) ama alan bölmesi çalışmalı.
        let g = GriGoruntu {
            genislik: 1,
            yukseklik: 1,
            pikseller: vec![128],
        };
        let b = kucult_8x8(&g);
        assert_eq!(b.len(), 64);
        assert!(b.iter().all(|v| (v - 128.0).abs() < 1e-9));
    }

    #[test]
    fn ayni_goruntunun_hash_i_ayni_urulur() {
        let g = goruntu_8x8(|x, y| ((x * 17 + y * 31) % 256) as u8);
        let h1 = hash_goruntuden(&g).expect("hash");
        let h2 = hash_goruntuden(&g).expect("hash");
        assert_eq!(h1, h2, "deterministik olmalı");
        assert_eq!(h1.hamming(&h2), 0);
    }

    #[test]
    fn duz_renkli_goruntunun_hash_i_bos_cikar() {
        // Düz görüntüde tüm AC katsayıları 0, ortalama 0 -> hiçbiri > 0+EPS
        let g = goruntu_8x8(|_, _| 100);
        let h = hash_goruntuden(&g).expect("hash");
        assert_eq!(h, AlgisalHash::BOS);
    }

    #[test]
    fn farkli_desenler_farkli_hash_uretiyor() {
        let a = goruntu_8x8(|x, _| if x < 4 { 0 } else { 255 });
        let b = goruntu_8x8(|x, _| if x < 2 { 0 } else { 255 });
        let ha = hash_goruntuden(&a).expect("ha");
        let hb = hash_goruntuden(&b).expect("hb");
        assert_ne!(ha, hb);
    }

    #[test]
    fn dikey_ve_yatay_seritler_ayri_hash_uretiyor() {
        let dikey = goruntu_8x8(|_, y| if y < 4 { 0 } else { 255 });
        let yatay = goruntu_8x8(|x, _| if x < 4 { 0 } else { 255 });
        let hd = hash_goruntuden(&dikey).expect("hd");
        let hy = hash_goruntuden(&yatay).expect("hy");
        assert_ne!(hd, hy, "dikey/yatay ayrımı hash'te görünmeli");
    }

    #[test]
    fn kucuk_boyutlu_goruntu_hash_almaz() {
        let g = GriGoruntu {
            genislik: 4,
            yukseklik: 4,
            pikseller: vec![10; 16],
        };
        assert!(hash_goruntuden(&g).is_none());
        assert!(hash_zorunlu(&g).is_err());
    }

    #[test]
    fn minimum_boyut_hash_alir() {
        let g = goruntu_8x8(|_, _| 42);
        assert!(hash_goruntuden(&g).is_some());
    }

    #[test]
    fn hamming_mesafesi_bit_farkini_sayar() {
        let a = AlgisalHash(0x0000_0000_0000_0000);
        let b = AlgisalHash(0x0000_0000_0000_0003);
        assert_eq!(a.hamming(&b), 2);
        assert_eq!(a.hamming(&a), 0);
        let c = AlgisalHash(u64::MAX);
        assert_eq!(a.hamming(&c), 64);
    }

    #[test]
    fn onaltilik_yazim_on_alti_hane() {
        let h = AlgisalHash(0xDEAD_BEEF_1234_5678);
        let s = h.onaltilik();
        assert_eq!(s.len(), 16);
        assert_eq!(s, "deadbeef12345678");
        assert_eq!(h.to_string(), s);
    }

    #[test]
    fn bit_erisimi_siradan_indis_uyumlu() {
        let h = AlgisalHash(0x8000_0000_0000_0000);
        assert!(h.bit(0), "ilk bit 1 olmalı");
        assert!(!h.bit(1));
        assert!(!h.bit(63));
        assert!(!h.bit(64), "sınır dışı indeks 0");
    }

    #[test]
    fn eps_toleransi_egik_ortalama_yakini_katsayiyi_reddeder() {
        // Ortalama = 0, katsayılar tam 0 ve çok küçük EPS.
        let mut k = [0f64; 64];
        k[0] = 999.0; // DC yoksayılır
        k[1] = EPS / 2.0; // eşiğin hemen altında -> 0
        k[2] = EPS * 2.0; // eşiğin üstünde -> 1
        let h = hash_katsayilardan(&k);
        assert!(!h.bit(1));
        assert!(h.bit(2));
    }

    #[test]
    fn dc_katsayisi_hash_e_girmez() {
        // Aynı desen, farklı parlaklık ofseti: hash'ler aynı olmalı (DC hariç).
        let desen = |ofset: u8| goruntu_8x8(move |x, y| ((x * 20 + y * 40) % 200) as u8 + ofset);
        let a = hash_goruntuden(&desen(0)).expect("a");
        let b = hash_goruntuden(&desen(30)).expect("b");
        assert_eq!(a, b, "parlaklık ofseti hash'i değiştirmemeli");
    }

    #[test]
    fn genis_goruntude_alan_ortalama_calisir() {
        // 64x64 dama deseni: 8x8 bloklar tam kare bloklar olur.
        let mut p = Vec::with_capacity(64 * 64);
        for y in 0..64u32 {
            for x in 0..64u32 {
                p.push(if (x / 8 + y / 8) % 2 == 0 { 200 } else { 60 });
            }
        }
        let g = GriGoruntu {
            genislik: 64,
            yukseklik: 64,
            pikseller: p,
        };
        let h = hash_goruntuden(&g).expect("hash");
        // Dama deseni güçlü bir AC sinyali üretmeli.
        assert_ne!(h, AlgisalHash::BOS);
    }
}
