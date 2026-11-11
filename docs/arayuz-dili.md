# Arayüz dili ve yazım kuralları

Klavye Stüdyo'daki her metin bu kurallara uyar: yeni metinler de, mevcutların düzeltmesi de. Amaç, büyük bir yazılım şirketinin ürününde beklenen sade, tutarlı ve güven veren dil (kullanıcının isteği, 2026-11-11).

Metinler `app/src/lib/i18n.ts`'te durur. Her anahtarın Türkçesi ve İngilizcesi vardır; arayüze doğrudan yazılmış metin olmaz (marka, dosya biçimi ve tuş adları hariç).

## 1. Genel ilkeler

- **Kısa.** Etiket 1–3 kelime. Açıklama en çok bir cümle (yaklaşık 90 karakter), yalnızca etiket kendini anlatmıyorsa.
- **Önce sonuç.** Kullanıcının ne yapacağını ya da ne olacağını söyle; iç işleyişi anlatma. "04 20 karesi", "RAM", "RGB565", "token" gibi teknik ayrıntı arayüze girmez.
- **Bir kez söyle.** Aynı bilgi bir sayfada bir yerde durur. Sayfa başlığının altındaki açıklama tek satırdır.
- **Sakin ton.** Ünlem yok, abartı yok ("harika", "kolayca", "sadece"), espri yok.
- **Doğru.** Metin uygulamanın gerçekte yaptığını söyler. Emin olunmayan bir şey kesinmiş gibi yazılmaz.

## 2. Türkçe

### Hitap
- **"Siz".** Açıklama ve yönlendirmelerde kibar emir ve iyelik: "Bir görsel seçin.", "Klavyenizi USB kablosuyla bağlayın.", "Görselleriniz".
- **Düğmeler fiil kökü:** Kaydet, Yükle, Dene, Vazgeç, Sil, Ekle, Uygula, Geri yükle. (Windows ve macOS Türkçesindeki gibi.)
- "Sen" kipi kullanılmaz ("aç", "seçtiğin", "görselin").

### Biçim
- **Cümle düzeni:** yalnız ilk harf büyük ("Tuş başına renk"). Tamamı büyük harf ve harf aralığı açılmış etiket yok. (Klavye ekranına çizilen kartların küçük başlıkları hariç.)
- **Noktalama:** Etiket, başlık ve düğmede nokta yok. Açıklama cümlesi noktayla biter.
- **"·" ile zincir yok.** Yalnız kısa bilgi satırlarında, en çok bir kez ("USB · 97 tuş" değil; ayrı ayrı yazılır ya da cümle kurulur).
- **İki nokta ile açıklama kalıbı yok** ("Kart: 2 yükleme sayılır" değil, "Her kart 2 yükleme sayılır.").
- **Üç nokta (…)** yalnız süren bir işlemde: "Yükleniyor…", "Okunuyor…".
- **Sayılar ve birimler:** %40, 5 sn, 2 dk, 14:00, 240×135, 2,8 sn (ondalık virgül).

### Hatalar
- Ne oldu + ne yapılmalı, en çok iki kısa cümle. Teknik kod ve İngilizce hata metni gösterilmez.
- Örnek: "Klavyeye ulaşılamadı. USB kablosunun takılı olduğundan emin olup tekrar deneyin."

### Yabancı kelime
- Türkçe karşılığı yerleşmiş olan kullanılır (TDK'ya bakılır). Marka, ürün, dosya biçimi ve tuş adları olduğu gibi kalır: USB, Bluetooth, Windows, GIF, PNG, Fn, Num Lock.

## 3. İngilizce

- Amerikan İngilizcesi ("color", "gray"), cümle düzeni ("Per-key color").
- "You/your". Düğmeler fiille başlar: Save, Upload, Try, Cancel, Delete.
- "Please", "successfully", ünlem yok. Hata: ne oldu + ne yapılmalı.
- Türkçeyle aynı bilgi, aynı uzunluk; birebir çeviri değil, doğal İngilizce.

## 4. Terim sözlüğü

| Kavram | Türkçe | English |
|---|---|---|
| Klavyenin kendi efektleri (firmware) | Hazır efektler | Built-in effects |
| Klavyenin yazılımı | klavye yazılımı | firmware |
| Kalıcı kayıt (flash) | klavyeye kaydet / kaydedilir | save to the keyboard / saved |
| Kaydetmeden gösterme (RAM) | önizleme / kaydedilmez | preview / not saved |
| Tuş başına tasarım | Tuş başına renk | Per-key color |
| Parlaklık, hız, yön | Parlaklık, Hız, Yön | Brightness, Speed, Direction |
| Ekrana gönderme | yükleme, Klavyeye yükle | upload, Upload to keyboard |
| Görsel arşivi | Kütüphane | Library |
| Klavyenin saat sayfası | Saat sayfası | Clock page |
| Klavyenin GIF sayfası | GIF sayfası | GIF page |
| Canlı kartlar | Canlı mod, Canlı ekran | Live mode, Live screen |
| Yükleme bütçesi | Günlük yükleme sınırı | Daily upload limit |
| Pomodoro | Odak zamanlayıcısı | Focus timer |
| Odak / kısa mola / uzun mola | Odak, Kısa mola, Uzun mola | Focus, Short break, Long break |
| Sistem göstergeleri | Sistem göstergeleri | System gauges |
| Hatırlatıcı | Hatırlatıcılar (sayfa), hatırlatma (tek) | Reminders, reminder |
| Tuş atama | Tuş atama, Fn katmanı | Key mapping, Fn layer |
| Makro | Makro | Macro |
| Gamma | gama | gamma |
| Kalibrasyon | Renk kalibrasyonu | Color calibration |
| Bağlantı modları | USB kablo, 2.4 GHz alıcı, Bluetooth | USB cable, 2.4 GHz receiver, Bluetooth |

## 5. Sayfa düzeni (görsel)

- Arayüz yazı tipi Windows'un kendi yazı tipi (Segoe UI Variable). Tek genişlikli yazı yalnız saat, kod ve onaltılık renk değerinde.
- Sayfa başı: başlık ve altında tek satır açıklama; sağda sayfanın ana eylemi. Başlığın üstünde küçük etiket yok.
- Bölüm başlığı küçük ve kalın, cümle düzeninde. Ayarlar bir kartın içinde satırlar hâlindedir: solda ad ve gerekirse tek cümle açıklama, sağda denetim.
- Kart içinde kart yok; ayrım çizgiyle yapılır. Parlama, doku ve degrade süs yok.
