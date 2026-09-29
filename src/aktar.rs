//! Dışa aktarma — seçilen kareleri kaynağa dokunmadan yeni klasöre kopyalar.
//!
//! # Güvenlik sözleşmesi (rapor `b03` kabul kriteri, `b07` şema 5)
//!
//! - **Kaynak dosyalara hiçbir işlem yapılmaz.** Yalnız okunur.
//! - Hedef, kaynak klasörün **içinde** olamaz (üst klasör veya alt klasör):
//!   bu durumda program kendini kopyalayabilir. [`hedef_uygun`] bunu reddeder.
//! - Kopyalama **geçici dosya üzerinden** yapılır: önce `hedef.gecici` yazılır,
//!   sonra `rename` ile hedefe taşınır. Yarım dosya bırakılmaz.
//! - Ad çakışmasında sayı eklenir (`IMG_0001.CR2` -> `IMG_0001-2.CR2`).

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::hata::{Hata, Sonuc};

/// Kopyalama sırasında kullanılan tampon boyutu (64 KiB).
pub const KOPYA_TAMPON: usize = 64 * 1024;

/// Hedef klasörün kaynakla çakışmadığını doğrular.
///
/// Hedef kaynağın altındaysa ya da kaynağın altındaysa reddedilir.
pub fn hedef_uygun(kaynak_kok: &Path, hedef: &Path) -> Sonuc<()> {
    let k = normalize(kaynak_kok);
    let h = normalize(hedef);
    if k == h {
        return Err(Hata::HedefCakisti {
            yol: hedef.to_path_buf(),
        });
    }
    if h.starts_with(&k) || k.starts_with(&h) {
        return Err(Hata::HedefCakisti {
            yol: hedef.to_path_buf(),
        });
    }
    Ok(())
}

/// Yolu karşılaştırma için sadeleştirir (küçük harf, son ayırıcı temizlenir).
///
/// Windows'ta büyük/küçük harf duyarsızdır; farklı platformlarda da
/// tutarlı davranış için normalize edilir.
fn normalize(yol: &Path) -> PathBuf {
    let mut s = yol.to_string_lossy().to_string();
    while s.len() > 1 && (s.ends_with('/') || s.ends_with('\\')) {
        s.pop();
    }
    PathBuf::from(s.to_lowercase())
}

/// Dosya adı çakışması varsa benzersiz ad üretir.
pub fn benzersiz_ad(hedef: &Path) -> PathBuf {
    if !hedef.exists() {
        return hedef.to_path_buf();
    }
    let ust = hedef.parent().map(Path::to_path_buf).unwrap_or_default();
    let ad = hedef
        .file_name()
        .map(|a| a.to_string_lossy().to_string())
        .unwrap_or_default();
    let (govde, uzanti) = match ad.rfind('.') {
        Some(i) if i > 0 => (ad[..i].to_string(), ad[i..].to_string()),
        _ => (ad.clone(), String::new()),
    };
    for n in 2..10_000u32 {
        let yeni = ust.join(format!("{}-{}{}", govde, n, uzanti));
        if !yeni.exists() {
            return yeni;
        }
    }
    hedef.to_path_buf()
}

/// Tek bir dosyayı geçici dosya üzerinden kopyalar ve hedefe taşır.
///
/// Kaynak dosya yalnız okunur. Hata olursa geçici dosya silinir.
pub fn kopyala(kaynak: &Path, hedef: &Path) -> Sonuc<u64> {
    let mut girdi = File::open(kaynak).map_err(|k| Hata::io("kaynak dosya açma", kaynak, k))?;
    let mut gecici_yol = hedef.as_os_str().to_os_string();
    gecici_yol.push(".gecici");
    let gecici = PathBuf::from(gecici_yol);

    let sonuc = (|| -> Sonuc<u64> {
        let mut cikti =
            File::create(&gecici).map_err(|k| Hata::io("geçici dosya oluşturma", &gecici, k))?;
        let mut tampon = vec![0u8; KOPYA_TAMPON];
        let mut toplam = 0u64;
        loop {
            let n = girdi
                .read(&mut tampon)
                .map_err(|k| Hata::io("kaynak dosya okuma", kaynak, k))?;
            if n == 0 {
                break;
            }
            cikti
                .write_all(&tampon[..n])
                .map_err(|k| Hata::io("geçici dosya yazma", &gecici, k))?;
            toplam += n as u64;
        }
        cikti
            .sync_all()
            .map_err(|k| Hata::io("geçici dosya boşaltma", &gecici, k))?;
        Ok(toplam)
    })();

    match sonuc {
        Ok(toplam) => {
            std::fs::rename(&gecici, hedef).map_err(|k| {
                let _ = std::fs::remove_file(&gecici);
                Hata::io("geçici dosyayı hedefe taşıma", hedef, k)
            })?;
            Ok(toplam)
        }
        Err(hata) => {
            // Yarım dosya bırakma.
            let _ = std::fs::remove_file(&gecici);
            Err(hata)
        }
    }
}

#[cfg(test)]
// Gerekçe: `unwrap`/`expect` yalnızca test içinde kullanılır.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    struct GeciciDizin {
        yol: PathBuf,
    }

    impl GeciciDizin {
        fn yeni(etiket: &str) -> GeciciDizin {
            let yol =
                std::env::temp_dir().join(format!("bj-aktar-{}-{}", etiket, std::process::id()));
            let _ = std::fs::remove_dir_all(&yol);
            std::fs::create_dir_all(&yol).expect("dizin");
            GeciciDizin { yol }
        }
    }

    impl Drop for GeciciDizin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.yol);
        }
    }

    #[test]
    fn ayni_klasor_reddedilir() {
        let d = GeciciDizin::yeni("ayni");
        assert!(hedef_uygun(&d.yol, &d.yol).is_err());
    }

    #[test]
    fn kaynagin_ici_reddedilir() {
        let d = GeciciDizin::yeni("ici");
        let alt = d.yol.join("secili");
        assert!(hedef_uygun(&d.yol, &alt).is_err());
    }

    #[test]
    fn kaynagi_icen_reddedilir() {
        let d = GeciciDizin::yeni("ust");
        let ust = d.yol.join("..");
        let kaynak = ust.join("kaynak");
        assert!(hedef_uygun(&kaynak, &d.yol).is_err());
    }

    #[test]
    fn ayri_klasor_kabul_edilir() {
        let a = std::env::temp_dir().join("bj-aktar-a-1");
        let b = std::env::temp_dir().join("bj-aktar-b-1");
        assert!(hedef_uygun(&a, &b).is_ok());
    }

    #[test]
    fn son_ayirac_temizlenir() {
        let a = std::env::temp_dir().join("bj-aktar-a-2");
        let a_ayri = PathBuf::from(format!("{}\\", a.display()));
        assert!(hedef_uygun(&a, &a_ayri).is_err());
    }

    #[test]
    fn kopyalama_bayt_bayt_ayni() {
        let d = GeciciDizin::yeni("kopya");
        let kaynak = d.yol.join("kaynak.bin");
        let icerik: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&kaynak, &icerik).expect("yaz");
        let hedef = d.yol.join("hedef.bin");
        let n = kopyala(&kaynak, &hedef).expect("kopya");
        assert_eq!(n, icerik.len() as u64);
        assert_eq!(std::fs::read(&hedef).expect("oku"), icerik);
    }

    #[test]
    fn kopyalama_kaynaga_dokunmaz() {
        let d = GeciciDizin::yeni("kaynakdegismez");
        let kaynak = d.yol.join("a.cr2");
        std::fs::write(&kaynak, b"icerik").expect("yaz");
        let once = std::fs::metadata(&kaynak).expect("meta").len();
        let hedef = d.yol.join("b.cr2");
        kopyala(&kaynak, &hedef).expect("kopya");
        let sonra = std::fs::metadata(&kaynak).expect("meta").len();
        assert_eq!(once, sonra);
        assert_eq!(std::fs::read(&kaynak).expect("oku"), b"icerik");
    }

    #[test]
    fn gecici_dosya_kalmaz() {
        let d = GeciciDizin::yeni("gecici");
        let kaynak = d.yol.join("a.bin");
        std::fs::write(&kaynak, b"x").expect("yaz");
        let hedef = d.yol.join("b.bin");
        kopyala(&kaynak, &hedef).expect("kopya");
        let gecici = d.yol.join("b.bin.gecici");
        assert!(!gecici.exists(), "geçici dosya kalmamalı");
    }

    #[test]
    fn olmayan_kaynak_hata_verir() {
        let d = GeciciDizin::yeni("yok");
        let hedef = d.yol.join("b.bin");
        assert!(kopyala(&d.yol.join("yok.bin"), &hedef).is_err());
    }

    #[test]
    fn benzersiz_ad_cakismada_numaralandirir() {
        let d = GeciciDizin::yeni("cakisma");
        let hedef = d.yol.join("a.cr2");
        assert_eq!(benzersiz_ad(&hedef), hedef);
        std::fs::write(&hedef, b"x").expect("yaz");
        let yeni = benzersiz_ad(&hedef);
        assert_ne!(yeni, hedef);
        assert!(yeni.to_string_lossy().contains("-2"));
    }

    #[test]
    fn benzersiz_ad_uzantisiz_dosyada_calisir() {
        let d = GeciciDizin::yeni("uzantisiz");
        let hedef = d.yol.join("dosya");
        std::fs::write(&hedef, b"x").expect("yaz");
        let yeni = benzersiz_ad(&hedef);
        assert!(yeni.to_string_lossy().ends_with("-2"), "{}", yeni.display());
    }

    #[test]
    fn toplu_disa_aktarma_calisir() {
        let d = GeciciDizin::yeni("toplu");
        let kaynak = d.yol.join("kaynak");
        let hedef = d.yol.join("hedef");
        std::fs::create_dir_all(&kaynak).expect("kaynak dizin");
        std::fs::create_dir_all(&hedef).expect("hedef dizin");
        for i in 0..5 {
            std::fs::write(kaynak.join(format!("a{}.cr2", i)), format!("dosya {}", i))
                .expect("yaz");
        }
        let secim = ["a0.cr2", "a2.cr2", "a4.cr2"];
        for ad in secim {
            let h = benzersiz_ad(&hedef.join(ad));
            kopyala(&kaynak.join(ad), &h).expect("kopya");
        }
        assert_eq!(std::fs::read_dir(&hedef).expect("oku").count(), 3);
        // Kaynak klasör bozulmamış olmalı.
        assert_eq!(std::fs::read_dir(&kaynak).expect("oku").count(), 5);
    }

    #[test]
    fn gecici_yol_kaynaga_eklenir() {
        let d = GeciciDizin::yeni("yol");
        let kaynak = d.yol.join("a.bin");
        std::fs::write(&kaynak, b"x").expect("yaz");
        let hedef = d.yol.join("b.bin");
        kopyala(&kaynak, &hedef).expect("kopya");
        assert!(hedef.exists());
    }
}
