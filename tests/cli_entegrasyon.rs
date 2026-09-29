//! Komut satırı (CLI) entegrasyon testleri.
//!
//! Testler gerçek `burstjudge` ikilisini `Command` ile çalıştırır; çıktı
//! biçimleri ve çıkış kodları doğrulanır. Yalnız `std::process` kullanılır,
//! ağ erişimi yoktur.

mod yardimci;

use std::process::Command;

use burstjudge::ornek_veri::Kayit;
use yardimci::{ham_yaz, ornek_yaz, GeciciDizin};

/// Test edilen ikilinin yolunu döndürür (`cargo test` bunu ayarlar).
fn ikili() -> std::path::PathBuf {
    let mut yol = std::env::current_exe().expect("mevcut yürütülebilir");
    // target/debug/deps/<test> -> target/debug/burstjudge(.exe)
    yol.pop();
    if yol.ends_with("deps") {
        yol.pop();
    }
    yol.join(format!("burstjudge{}", std::env::consts::EXE_SUFFIX))
}

fn calistir(args: &[&str]) -> (bool, String, String) {
    let cikti = Command::new(ikili())
        .args(args)
        .output()
        .expect("ikili calistir");
    (
        cikti.status.success(),
        String::from_utf8_lossy(&cikti.stdout).to_string(),
        String::from_utf8_lossy(&cikti.stderr).to_string(),
    )
}

#[test]
fn yardim_ciktisi_basarili_dondurur() {
    let (ok, std_out, _) = calistir(&["--help"]);
    assert!(ok, "--help başarısız");
    assert!(std_out.contains("burstjudge"));
    assert!(std_out.contains("group"));
    assert!(std_out.contains("judge"));
    assert!(std_out.contains("export"));
    assert!(std_out.contains("info"));
}

#[test]
fn surum_bayragi_calisir() {
    let (ok, std_out, _) = calistir(&["--version"]);
    assert!(ok);
    assert!(std_out.contains(env!("CARGO_PKG_VERSION")), "{}", std_out);
}

#[test]
fn info_gecerli_dosyada_metadata_yazar() {
    let d = GeciciDizin::yeni("cli-info").expect("dizin");
    let yol = ornek_yaz(&d, "IMG_0001.CR2", &Kayit::onizleme_ile(16, 16, 128));
    let (ok, std_out, _) = calistir(&["info", yol.to_str().expect("utf8")]);
    assert!(ok, "info başarısız");
    assert!(std_out.contains("Test Kamera"), "{}", std_out);
    assert!(std_out.contains("ISO 200"), "{}", std_out);
    assert!(std_out.contains("hash         : "), "{}", std_out);
    // Kısmi okuma kanıtı çıktıda görünür olmalı.
    assert!(std_out.contains("okunan"), "{}", std_out);
}

#[test]
fn info_bozuk_dosyada_hata_kodu_dondurur() {
    let d = GeciciDizin::yeni("cli-info-bozuk").expect("dizin");
    let yol = ham_yaz(&d, "bozuk.cr2", b"bu bir TIFF degil");
    let (ok, _, hata) = calistir(&["info", yol.to_str().expect("utf8")]);
    assert!(!ok, "bozuk dosyada başarı beklenmez");
    assert!(hata.contains("hata:"), "{}", hata);
    assert!(hata.contains("TIFF"), "{}", hata);
}

#[test]
fn group_bos_klasorde_basarili_dondurur() {
    let d = GeciciDizin::yeni("cli-grup-bos").expect("dizin");
    let (ok, std_out, _) = calistir(&["group", d.yol().to_str().expect("utf8")]);
    assert!(ok, "{}", std_out);
    assert!(std_out.contains("grup taraması"), "{}", std_out);
    assert!(std_out.contains("okunan bayt"), "{}", std_out);
    assert!(std_out.contains("dosya         : 0"), "{}", std_out);
}

#[test]
fn group_burst_klasorunde_grup_olusturur() {
    let d = GeciciDizin::yeni("cli-grup").expect("dizin");
    for i in 0..3 {
        ornek_yaz(
            &d,
            &format!("IMG_000{}.CR2", i),
            &Kayit::desenli_onizleme_ile(burstjudge::jpeg_test_veri::Desen::Damali {
                hucre: 8,
                koyu: 40,
                acik: 210,
            }),
        );
    }
    let (ok, std_out, _) = calistir(&["group", d.yol().to_str().expect("utf8")]);
    assert!(ok, "{}", std_out);
    assert!(std_out.contains("dosya         : 3"), "{}", std_out);
    assert!(std_out.contains("önizleme      : 3 bulundu"), "{}", std_out);
    assert!(
        std_out.contains("hash          : 3 üretildi"),
        "{}",
        std_out
    );
    assert!(std_out.contains("grup          : 1"), "{}", std_out);
}

#[test]
fn group_bozuk_dosyada_taramayi_durdurmaz() {
    let d = GeciciDizin::yeni("cli-grup-bozuk").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::canon());
    ham_yaz(&d, "bozuk.cr2", b"xx");
    let (ok, std_out, _) = calistir(&["group", d.yol().to_str().expect("utf8")]);
    assert!(ok, "bozuk dosyada tarama durmamalı");
    assert!(std_out.contains("dosya         : 2"), "{}", std_out);
    assert!(std_out.contains("atlanan       : 1"), "{}", std_out);
}

#[test]
fn group_gecersiz_esik_degerini_reddeder() {
    let d = GeciciDizin::yeni("cli-esik").expect("dizin");
    let (ok, _, hata) = calistir(&["group", d.yol().to_str().expect("utf8"), "--esik", "99"]);
    assert!(!ok, "esik 99 reddedilmeli");
    assert!(hata.contains("0-64"), "{}", hata);
}

#[test]
fn judge_json_raporu_yazar_ve_oneri_icerir() {
    let d = GeciciDizin::yeni("cli-judge").expect("dizin");
    ornek_yaz(
        &d,
        "kotu.cr2",
        &Kayit {
            iso: Some(12800),
            onizleme: Some((16, 16, 128)),
            ..Kayit::canon()
        },
    );
    ornek_yaz(
        &d,
        "iyi.cr2",
        &Kayit {
            iso: Some(100),
            poz_suresi: Some(1.0 / 2000.0),
            onizleme: Some((16, 16, 128)),
            ..Kayit::canon()
        },
    );
    let cikti = d.dosya("rapor.json");
    let (ok, _, hata) = calistir(&[
        "judge",
        d.yol().to_str().expect("utf8"),
        "--cikti",
        cikti.to_str().expect("utf8"),
    ]);
    assert!(ok, "{}", hata);
    assert!(cikti.exists(), "rapor dosyası oluşmalı");

    let metin = std::fs::read_to_string(&cikti).expect("rapor oku");
    let r: serde_json::Value = serde_json::from_str(&metin).expect("json ayristir");
    assert_eq!(r["sema"], 1);
    assert_eq!(r["esik"], 8);
    assert_eq!(r["gruplar"].as_array().expect("gruplar").len(), 1);
    let oneri = r["gruplar"][0]["oneri"]
        .as_str()
        .expect("oneri yolu")
        .to_string();
    assert!(oneri.ends_with("iyi.cr2"), "öneri: {}", oneri);
    // Bileşen puanları raporda görünür olmalı.
    let bilesenler = r["gruplar"][0]["uyeler"][0]["puan"]["bilesenler"]
        .as_array()
        .expect("bilesenler");
    assert!(bilesenler.len() >= 4, "en az 4 bileşen");
}

#[test]
fn judge_ayar_dosyasini_dogrular() {
    let d = GeciciDizin::yeni("cli-ayar").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::canon());
    let ayar = d.dosya("ayar.json");
    std::fs::write(
        &ayar,
        r#"{"agirlik_keskinlik": 0.5, "agirlik_pozlama": 0.5}"#,
    )
    .expect("ayar yaz");
    let (ok, _, hata) = calistir(&[
        "judge",
        d.yol().to_str().expect("utf8"),
        "--ayar",
        ayar.to_str().expect("utf8"),
    ]);
    assert!(ok, "geçerli ayar kabul edilmeli: {}", hata);
}

#[test]
fn judge_gecersiz_ayar_dosyasini_reddeder() {
    let d = GeciciDizin::yeni("cli-ayar-bozuk").expect("dizin");
    ornek_yaz(&d, "a.cr2", &Kayit::canon());
    let ayar = d.dosya("bozuk.json");
    std::fs::write(&ayar, r#"{"agirlik_keskinlik": -1.0}"#).expect("ayar yaz");
    let (ok, _, hata) = calistir(&[
        "judge",
        d.yol().to_str().expect("utf8"),
        "--ayar",
        ayar.to_str().expect("utf8"),
    ]);
    assert!(!ok, "negatif ağırlık reddedilmeli");
    assert!(hata.contains("ayar"), "{}", hata);
}

#[test]
fn export_onerileri_kopyalar_kaynaga_dokunmaz() {
    let d = GeciciDizin::yeni("cli-export").expect("dizin");
    let kaynak = d.dosya("kaynak");
    std::fs::create_dir_all(&kaynak).expect("kaynak");
    // Üç farklı desen -> üç ayrı grup -> üç kere dışa aktarım.
    let desenler = [
        burstjudge::jpeg_test_veri::Desen::DikeySerit {
            periyot: 8,
            koyu: 30,
            acik: 200,
        },
        burstjudge::jpeg_test_veri::Desen::Damali {
            hucre: 8,
            koyu: 40,
            acik: 210,
        },
        burstjudge::jpeg_test_veri::Desen::Radyal {
            merkez_gri: 240,
            kenar_gri: 25,
        },
    ];
    for (i, desen) in desenler.iter().enumerate() {
        std::fs::write(
            kaynak.join(format!("IMG_000{}.CR2", i)),
            burstjudge::ornek_veri::tif_tek(&Kayit::desenli_onizleme_ile(*desen)),
        )
        .expect("yaz");
    }
    let rapor = d.dosya("rapor.json");
    let hedef = d.dosya("secili");

    let (ok1, _, h1) = calistir(&[
        "judge",
        kaynak.to_str().expect("utf8"),
        "--cikti",
        rapor.to_str().expect("utf8"),
    ]);
    assert!(ok1, "{}", h1);

    let (ok2, std_out, h2) = calistir(&[
        "export",
        rapor.to_str().expect("utf8"),
        hedef.to_str().expect("utf8"),
    ]);
    assert!(ok2, "{}", h2);
    assert!(std_out.contains("toplam"), "{}", std_out);
    assert_eq!(std::fs::read_dir(&hedef).expect("oku").count(), 3);
    // Kaynak klasör hâlâ 3 dosya.
    assert_eq!(std::fs::read_dir(&kaynak).expect("oku").count(), 3);
}

#[test]
fn export_hedefi_kaynakla_ayni_olursa_reddedilir() {
    let d = GeciciDizin::yeni("cli-export-cakisma").expect("dizin");
    let kaynak = d.dosya("kaynak");
    std::fs::create_dir_all(&kaynak).expect("kaynak");
    ornek_yaz(&d, "a.cr2", &Kayit::canon());
    std::fs::write(
        kaynak.join("a.cr2"),
        burstjudge::ornek_veri::tif_tek(&Kayit::canon()),
    )
    .expect("yaz");
    let rapor = d.dosya("rapor.json");
    let (ok1, _, _) = calistir(&[
        "judge",
        kaynak.to_str().expect("utf8"),
        "--cikti",
        rapor.to_str().expect("utf8"),
    ]);
    assert!(ok1);
    // Hedef = kaynak -> reddedilmeli.
    let (ok2, _, hata) = calistir(&[
        "export",
        rapor.to_str().expect("utf8"),
        kaynak.to_str().expect("utf8"),
    ]);
    assert!(!ok2, "kaynakla aynı hedef reddedilmeli");
    assert!(hata.contains("içinde olamaz"), "{}", hata);
}

#[test]
fn info_onizlemeyi_ayri_dosyaya_yazabilir() {
    let d = GeciciDizin::yeni("cli-onizleme").expect("dizin");
    let kaynak = ornek_yaz(&d, "a.cr2", &Kayit::onizleme_ile(16, 16, 128));
    let hedef = d.dosya("onizleme.jpg");
    let (ok, std_out, hata) = calistir(&[
        "info",
        kaynak.to_str().expect("utf8"),
        "--onizleme-yaz",
        hedef.to_str().expect("utf8"),
    ]);
    assert!(ok, "{}", hata);
    assert!(std_out.contains("bayt yazıldı"), "{}", std_out);
    assert!(hedef.exists(), "önizleme dosyası oluşmalı");
    let baytlar = std::fs::read(&hedef).expect("oku");
    assert!(burstjudge::onizleme::jpeg_soi_dogrula(&baytlar));
    assert!(
        burstjudge::jpeg::coz_jpeg(&baytlar).is_ok(),
        "yazılan JPEG çözülebilmeli"
    );
}

#[test]
fn info_onizlemesiz_dosyada_hata_verir() {
    let d = GeciciDizin::yeni("cli-onizleme-yok").expect("dizin");
    let kaynak = ornek_yaz(&d, "a.cr2", &Kayit::oran());
    let hedef = d.dosya("onizleme.jpg");
    let (ok, _, hata) = calistir(&[
        "info",
        kaynak.to_str().expect("utf8"),
        "--onizleme-yaz",
        hedef.to_str().expect("utf8"),
    ]);
    assert!(!ok, "önizlemesiz dosyada başarı beklenmez");
    assert!(hata.contains("önizleme"), "{}", hata);
}

#[test]
fn turkce_ve_bosluk_iceren_yol_calisir() {
    let d = GeciciDizin::yeni("cli-yol").expect("dizin");
    let alt = d.dosya("çekim 2026-09-29");
    std::fs::create_dir_all(&alt).expect("dizin");
    std::fs::write(
        alt.join("IMG_0001.CR2"),
        burstjudge::ornek_veri::tif_tek(&Kayit::canon()),
    )
    .expect("yaz");
    let (ok, std_out, hata) = calistir(&["group", alt.to_str().expect("utf8")]);
    assert!(ok, "Türkçe karakterli yol başarısız: {}", hata);
    assert!(std_out.contains("dosya         : 1"), "{}", std_out);
}
