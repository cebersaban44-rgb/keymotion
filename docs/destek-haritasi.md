# Destek haritası: klavyeler, bağlantı modları ve özellikler

Bu belge Klavye Stüdyo'nun hangi klavyeleri tanıdığını, her bağlantı modunda neyin çalıştığını ve başka hangi klavyelerin sonradan eklenebileceğini anlatır. İlk sürümde README ve GitHub sayfası bu belgeden hazırlanacak (2026-11-11).

İşaretler:

- ✅ Uygulamada var ve bu klavyede denendi.
- 🔜 Klavye bunu bu modda destekliyor görünüyor (başka projelerin ölçümleri), ama uygulamada henüz yok.
- ❌ Bu modda mümkün değil.

## 1. Desteklenen klavyeler

| Klavye | Durum |
|---|---|
| **AJAZZ AK980 PRO** (ekranlı, topuzlu, üç modlu) | ✅ Tam destek. Bütün özellikler kullanıcının klavyesinde denendi (ürün yazılımı 0x0106). |
| AJAZZ "AK980 V2" | Bazı satıcılar ve projeler aynı klavyeye bu adı veriyor; USB kimliği ve ürün adı birebir aynı. Uygulama onu AK980 PRO olarak tanır ve aynı şekilde çalışması beklenir. Bizde ayrıca denenmedi. |

Uygulama şu an yalnızca bu klavyeyi tanır. Başka bir klavye takıldığında ona hiçbir komut gönderilmez ve uygulama "Klavye bekleniyor" der.

## 2. Uygulama klavyeyi nasıl tanıyor

Her model, `crates/kb-devices/models/` altında bir dosyayla tanımlanır. AK980 PRO'nun dosyası `ajazz-ak980-pro.toml`. Uygulama her 2 saniyede bir HID aygıtlarını tarar ve modele şu kurallarla eşler:

| Bağlantı | Uygulamanın baktığı şey |
|---|---|
| USB kablo | VID `0C45`, PID `8009`, USB bus, ürün adında "AK980" (bu klavye "AK980 PRO" diyor). Komut kanalı `FF13`, ekran kanalı `FF68`. |
| 2.4 GHz alıcı | VID `0C45`, PID `FEFE`, USB bus, üreticiye özel `FF60` kanalı. |
| Bluetooth | Eşleştirilmiş Bluetooth aygıtının adı "AK980 PRO" ile başlıyor (ör. "AK980 PRO-1"; son ek kanal numarası). Windows'ta aynı aygıt `0C45:FEFE` olarak, Bluetooth bus'ında görünür. |

Neden bu kadar sıkı:

- `0C45:8009` ve `0C45:800A` SONiX'in genel kimlikleri. En az on farklı klavye bunları kullanıyor ve bunların bir kısmı **başka bir komut ailesi** konuşuyor (bölüm 7). Yanlış aileye gönderilen bir ekran komutu, ekrandaki görsel yuvasını silebiliyor.
- `FEFE` bu klavyede 2.4 GHz alıcının kimliği, AK820 ve AK35I'da ise Bluetooth'un. Bu yüzden alıcı ile Bluetooth bus türüne göre ayrılıyor.
- **Yeni (2026-11-11):** Uygulama bir kanalı açarken HID tanımlayıcısının şeklini de kontrol ediyor. Komut kanalı 64 baytlık raporlar, ekran kanalı 4096 baytlık çıkış raporu taşımalı. Şekil farklıysa kanal kullanılmaz ve klavyeye hiçbir şey gönderilmez (`crates/kb-core/src/shape.rs`). Kullanıcının klavyesinden okunan tanımlayıcılar testlerde duruyor.

## 3. Bağlantı modları ve geçiş

- **Mod, klavyenin arkasındaki anahtarla seçilir:** soldan sağa **2.4G · USB · BT**. USB ortadadır (fotoğrafla doğrulandı).
- **Uygulama modu değiştiremez.** Bunu yapan bir komut bilinmiyor; resmi sürücüde de yok. Uygulamadaki 2.4G / USB / BT çizimi bir seçici değil, anahtarın konumunu gösterir. Bağlantı sayfası şu anki bağlantıyı, anahtarın konumunu ve bölüm 4'teki tabloyu gösterir.
- Bluetooth'ta üç kanal var. Kılavuza göre Fn+Q / W / E kanallar arasında geçer, 3 saniye basılı tutmak eşleştirmeyi başlatır. Fn+R (3 sn) 2.4 GHz eşleştirmesidir. Bu kısayollar bu klavyede henüz denenmedi (`teknik-analiz.md` §3).
- 2.4G ve Bluetooth modunda kablo yalnızca şarj eder. Kaynaklara göre klavyenin USB arayüzleri bu modlarda görünmez, uygulama da klavyeyi yalnızca kablosuz bağlantı üzerinden görür.

## 4. Özellikler ve bağlantı modları

| Özellik | USB kablo | 2.4 GHz alıcı | Bluetooth |
|---|---|---|---|
| Yazma, medya tuşları, topuz | ✅ | ✅ | ✅ |
| Işık efektleri (20 hazır efekt, renk, parlaklık, hız, yön) | ✅ | 🔜 | ❌ |
| Tuş başına renk | ✅ | 🔜 (yalnızca statik analiz) | ❌ |
| Tuş atama ve Fn katmanı | ✅ | 🔜 (riskli; dikkatli ölçüm gerekir) | ❌ |
| Makrolar | ✅ | 🔜 (riskli; dikkatli ölçüm gerekir) | ❌ |
| Ekranda görsel, GIF, yazı kartı, slayt | ✅ | 🔜 (kardeş klavyede bir yükleme yaklaşık 62 sn) | ❌ |
| Saat eşitleme | ✅ | 🔜 | ❌ |
| Genel Bakış'ta canlı klavye görünümü (ışığın aynası) | ✅ | ❌ (bu modda geri okuma yok) | ❌ |
| Canlı ışık: odak zamanlayıcısı, sistem göstergeleri, hatırlatıcı dalgası, ışık önizlemesi | ✅ | ❌ (2.4G'de tuş başına gerçek zamanlı kare yok) | ❌ |
| Canlı kartlar: çalan şarkı, odak, takvim, hatırlatıcı | ✅ | ❌ (yükleme çok yavaş) | ❌ |
| Pil düzeyi | Son Bluetooth okuması (USB'de klavye pil bildirmiyor) | 🔜 | ✅ |
| Klavye seçenekleri (uyku, Win kilidi) | 🔜 (uygulamada henüz yok) | 🔜 | ❌ |
| Renk kalibrasyonu (test desenleri) | ✅ | ❌ | ❌ |

Uygulamanın kendi içinde çalışanlar her modda çalışır, yalnızca klavyeye yansımaz. Odak zamanlayıcısı süreyi sayar, hatırlatıcı listesi ve takvim okunur, ekran kütüphanesine görsel hazırlanıp kaydedilebilir, tuş atama ve makrolar düzenlenip USB'ye geçince kaydedilebilir.

Notlar:

- **Bluetooth:** Ölçüldü (2026-11-10). Klavyenin Bluetooth'ta üreticiye özel bir kanalı yok, yalnızca standart HID ve pil servisi var. Bu bir donanım sınırı; uygulama ne yaparsa yapsın Bluetooth'ta ayar yapılamaz. `FEE0/FEE1` (ürün yazılımı güncelleme kanalı) asla yazılmaz.
- **2.4 GHz alıcı:** Uygulamada sürücüsü yok. Başka projeler AK980 için pil (`20 01`) ve saat (`0C 10`) komutlarını doğrulamış; ışık, tuş atama ve ekran komutları yalnızca kardeş klavyelerde görülmüş. Resmi sürücünün yapılandırması (`menu_wireless`) alıcı üzerinden makro, ışık efekti, tuş başına renk ve müzik ışığı sunuyor, ekranı sunmuyor. Kullanıcıyla kararlaştırıldığı gibi 2.4G ve Bluetooth işi, bütün özellikler bittikten sonra ele alınacak (`DURUM.md`, "Sona bırakılan").
- **USB'de pil:** Klavyenin yazılımı USB'de pil düzeyi bildirmiyor (bölüm 5). Uygulama son Bluetooth okumasını yaşıyla birlikte gösteriyor ve klavyenin şarj olduğunu Fn ışığından anlıyor.

## 5. Pil: USB'de neden yok, ne yapılabilir

Kullanıcının sorusu (2026-11-11): "Resmi sürücüyü indirmiştik. Kendimiz bir katman ya da modül yazıp USB'de pili okuyabilir miyiz?"

Bilgisayar tarafına yazılacak bir katman, klavyenin göndermediği bir bilgiyi okuyamaz. Bu yüzden önce klavyenin USB'de pil bilgisini herhangi bir yoldan verip vermediğine bakıldı:

| Bakılan yer | Sonuç |
|---|---|
| Resmi sürücünün yapılandırması (indirilen paketteki `config.xml`, `device-config.xml`) | Yalnızca USB ve 2.4G modu tanımlı. USB için pil ayarı yok; pil 2.4G alıcıdaki `20 01` ile okunuyor (stok sürücü 15 saniyede bir soruyor). |
| Başka projelerin ürün yazılımı incelemesi (kardeş AK35I, V1.17) | Ürün yazılımının USB tarafında pil kodu yok (`research/protokol-ff13.md` §3, C15). |
| Kablolu `20 01` deneyen projeler | Gelen `FF`, pil değil "tanınmayan komut" yankısı. Göndermiyoruz. |
| Klavyenin USB HID tanımlayıcıları | Pil kullanımı (Battery Strength) yok. Windows'un pil göstergesi de bu yüzden okuyamaz. |
| **Pasif dinleme (yeni, 2026-11-11, `kb listen`)** | Komut (`FF13`) ve üreticiye özel `FFFF` koleksiyonu 20 saniye dinlendi, hiçbir şey gönderilmedi. Klavye kendiliğinden **hiç rapor göndermedi**. |
| **Fn ışığı (yeni)** | Ürün yazılımı şarj olurken Fn tuşunu kırmızı yakıyor (2026-11-10, ışık kapalıyken bile). Bu ışık USB'deki renk okumasıyla (`04 F5`) görülebiliyor. Uzun süre takılı kalan klavyede 2026-11-11'de Fn sönüktü; büyük olasılıkla şarj bitmişti. |

Sonuç ve yapılanlar:

- **USB'de pil yüzdesi okunamaz.** Bu bir uygulama eksikliği değil; klavyenin yazılımı bu bilgiyi kabloda dışarı vermiyor. Resmi sürücü de kabloda pil göstermiyor.
- **Yapıldı: "Şarj oluyor" bilgisi.** Genel Bakış, Fn ışığı kırmızıyken (ve yanındaki tuşlar kırmızı değilken, yani kırmızı bir efekt değilse) "Klavye şu an şarj oluyor." diyor. Okuma zaten çalışan canlı aynadan geliyor, klavyeye yeni bir komut gitmiyor.
- **Kullanıcıyla doğrulanacak:** Fn'nin şarj bitince söndüğü. Doğrulanırsa kart "Şarj tamamlandı" da diyebilir.
- **İleride: 2.4 GHz alıcıyla gerçek yüzde.** Alıcının `20 01` komutu AK980'de başkalarınca doğrulanmış; alıcı sürücüsü yazılınca pil yüzdesi o modda canlı okunur.
- **Yapılmayacaklar:** Yalnızca klavyenin yazılımını değiştirmek USB'de yüzdeyi verebilirdi; projenin kuralı gereği klavyeye yazılım (firmware) gönderilmez. Henüz denenmemiş okuma komutları (`04 10/12/16/22/26`) yanlış çıkarsa ayarları silebilir, bu yüzden ayrı bir risk değerlendirmesi ve kullanıcı onayı olmadan denenmez.

## 6. Eklenebilecek klavyeler (aynı komut ailesi)

Aşağıdaki klavyeler aynı SONiX "04 xx" komut ailesini konuşuyor görünüyor. Komut kanalı 64 baytlık `FF13`, ekranı olanlarda 4096 baytlık `FF68`. Her birini eklemek için bir model dosyası (kimlik, ürün adı, ekran boyutu, kare sınırı, tuş ve LED haritası) ve **o klavyede denenmiş bir doğrulama** gerekir. Elimizde bu klavyeler olmadığı için şimdilik hiçbiri desteklenmiyor.

| Klavye (başka adları) | USB kimliği ve ürün adı | Ekran | Kanıt |
|---|---|---|---|
| AJAZZ AK820 Pro (EPOMAKER AK820 Pro), FF13 yazılımlı sürüm | `0C45:8009` / `800A`, "AK820" | 128×128 | Donanımda doğrulanmış (wsclx/ak820pro-modder) |
| AJAZZ AK820 MAX / MAX PLUS (SONiX ekranlı sürüm) | `0C45:8009` / `800A`, "AK820 MAX" | 128×128 | Donanımda (BakulBd/OpenKey) |
| AJAZZ AK35I V3 MAX | `0C45:8009`, "USB KEYBOARD" (genel ad) | 240×135 | Donanımda (Ike-li) |
| AJAZZ AKS075 | `0C45:8009` | 128×128 | Donanımda, ekran ve saat (aar-rafi) |
| AJAZZ AK650 / AK650 Pro | `05AC:024F` (kablolu!), "AK650" | 0,85" | Yalnızca saat (geraeldo) |
| AULA F108 Pro | `0C45:800A`, "AULA F108Pro" | 240×135, 141 kare | Donanımda (parsiya, OpenKey) |
| AULA F75 Max (EPOMAKER x AULA) | `0C45:800A`, "AULA F75Max" | 128×128, 255 kare | Donanımda (VitalyArt, w33ble) |
| AULA EA75 Max | `0C45:800A`, "AULA EA75MAX" | 135×240 (dikey) | Donanımda (pedrovrima) |
| AULA NOVA75 | `0C45:800A`, "AULA-NOVA75" | 240×135, 107 kare | Donanımda, saat ve yükleme |
| AULA S75 Pro | `0C45:800A` | 135×240 | Tek kaynak, statik |
| AULA F98 Pro | `0C45:800A` | 1,14" | Yalnızca ışık, statik |
| LangTu LT-104 | `0C45:8009` | 160×80 | Donanımda (yazarı) |
| AJAZZ AK980 (ekransız, 101 tuş, kablolu) | `0C45:8006` | yok | Yalnızca ışık yakalaması (OpenRGB #5772) |

Dikkat:

- Ekran boyutu ve kare sınırı modelden modele değişiyor (240×135, 135×240, 128×128, 160×80; 107, 140, 141, 255 kare). Kare sınırını aşmak AULA F108 Pro'da ekran menüsü grafiklerini kalıcı olarak bozdu. Bu yüzden her model kendi dosyasıyla ve ölçümle eklenmeli.
- Aynı komut kodu modelden modele farklı anlama gelebiliyor (`04 17` AK820'de sistem ayarları, LT-104'te ışık şeridi).
- Ürün adı genel olan klavyeler ("USB KEYBOARD") kendiliğinden tanınmamalı.

Öneri sırası (P4): AK820 Pro ve AULA F108 Pro (en çok belgelenmiş olanlar), ardından AK35I V3 MAX. Her biri için o klavyeye sahip bir kullanıcının denemesi gerekiyor.

## 7. Benzer görünen ama uyumsuz klavyeler

Bu klavyeler başka bir komut ailesi ("AA" çerçeveleri ya da başka çipler) kullanıyor. Uygulama bunlara hiçbir zaman komut göndermemeli; yeni şekil kontrolü de bunu güvenceye alıyor.

| Klavye | Kimlik | Neden uyumsuz |
|---|---|---|
| AJAZZ AK980 MAX | `0C45:8043`, `8050–8054` | "AA" ailesi |
| AJAZZ AK820 Pro, "AA" yazılımlı sürüm (fw 1.07, 1.16) | `0C45:8009` / `800A` | "AA" ailesi; `04 72` görsel yuvasını siliyor |
| AJAZZ AK820 MAX web sürücülü seri (MAX HE dahil) | `0C45:8033`, `8044`, `8070`, `809B`, `80A0`, `80B0`, `80B1`, `80B9` | "AA" ailesi |
| AJAZZ AK820 Max Plus (CRS / SinoWealth sürümü) | `1A2C:A036`, `258A:010C` | Başka çip |
| AJAZZ AK820 (Pro olmayan) | `320F:505B` | EVision |
| AULA F75, F87, F87 Pro, F99, F99 Pro | `258A:010C` | SinoWealth |
| AULA F87S 8K, Nova98 | `38A6:2908`, `38A6:273B` | AA/55 |
| AJAZZ AK680 V2 / MAX | web sürücüsü | AA |
| AJAZZ AK992 | `1A2C:9C00`, `320F:50C2` | Başka çip |
| AJAZZ AKS068 | — | VIA/QMK |

Bilinmeyenler: AK870 V2 (ekranlı), AK820 MAX Ultra, AK870/873/966, AULA F99 Max. Açık kaynakta belgelenmemiş.

## 8. Bu incelemede düzeltilenler (2026-11-11)

- **2.4 GHz alıcı takılınca yanlış açık görünen sayfalar.** Model dosyası alıcı için ışık, saat ve pil sayıyordu ama uygulamada alıcı sürücüsü yok. Işıklar sayfası kilitsiz görünüyor, saat eşitleme düğmesi çalışır gibi duruyordu. Artık uygulama her modda yalnızca gerçekten yapabildiğini sunuyor (`kb_core::app_capabilities`); alıcı modunda bütün ayar sayfaları kilitli ve "USB kablosu gerekir" diyor.
- **"Seçenekler" özelliği var gibi görünüyordu.** Genel Bakış'taki özellik listesinde "Uyku, Win kilidi ve diğer seçenekler" işaretliydi, ama uygulamada bu ayarlar henüz yok. Listeden çıkarıldı.
- **Şekil kontrolü.** Komut ve ekran kanalı açılırken HID tanımlayıcısının şekli kontrol ediliyor (bölüm 2).
- **Arka uç hata metni.** "Klavye USB modunda değil" hatası arka uçta Türkçe yazılmıştı; arayüz onu çeviremiyor ve genel bir hata gösteriyordu. Artık İngilizce kod olarak gidiyor, arayüz kendi iki dilli metnini gösteriyor.
- **Bağlantı sayfası.** Kenar çubuğundaki "Bağlantı" Genel Bakış'ı tekrar açıyordu ve 2.4G / USB / BT göstergesi seçici gibi görünüyordu. Artık kendi sayfası var: şu anki bağlantı, anahtarın konumu ("Modu bu anahtarla değiştirirsiniz") ve bölüm 4'teki tablo. Tablo, uygulamanın her modda sunduğu özelliklerden kendiliğinden oluşuyor; alıcı sürücüsü yazılınca kendisi güncellenir. Genel Bakış'taki kart sadeleşip bu sayfaya bağlantı veriyor.
- **Şarj durumu.** Bölüm 5.

## 9. Açık sorular

- Fn ışığı şarj bitince sönüyor mu (bölüm 5)?
- Bluetooth kanal ve eşleştirme kısayolları (Fn+Q/W/E, Fn+R) bu klavyede denenmedi.
- Bluetooth modunda kablo takılıyken klavyenin USB arayüzleri gerçekten kayboluyor mu? 2.4G için kaynaklar evet diyor, BT için ölçülmedi.
- Bir AK980 V2 projesi (d991d/ajazz-control) ekran yüklemesinin yuva baytı için `03` kullanıyor; biz `01` kullanıyoruz ve kullanıcının klavyesinde 33 yükleme sorunsuz çalıştı. Fark ürün yazılımı sürümünden olabilir.
- Tanınmayan bir SONiX klavye takıldığında "Klavye bekleniyor" yerine "Bu klavye henüz desteklenmiyor" demek (kullanıcıya önerilecek).

## Kaynaklar

Ayrıntılı kaynak listesi: `research/protokol-ff13.md` (S1–S12, W1–W9). Bu belgeye eklenenler (2026-11-11 araştırması): d991d/ajazz-control, BakulBd/OpenKey (`devices/*.json`, `docs/COMPATIBILITY.md`, `docs/MODEL_GUIDE.md`), wsclx/ak820pro-modder (`docs/research/2026-10-25-board-scan.md`), geraeldo/ajazz-ak650-clock, VitalyArt/Aula-F75-Max-Driver, w33ble/aula-f75-max-control, pedrovrima/aula-ea75max, thibaultmiclo/aula-driver, fjhfelix/nova75-studio, NMDx86/aula-s75-gif-uploader, NollieL/SignalRgb_CN_Key, iiaiiappa/lt104-keyboard-linux, kidush/omarchy-aula-f108pro, arafatamim/OpenRGBAK820Plugin, xero/aks068-via; OpenRGB issues #4577, #4595, #5772, #5966.
