# Gizlilik

Bu masaüstü uygulaması klavyenizi yönetir (ışıklar, tuş atama, makrolar, ekran ve saat) ve klavyede bilgi gösterir. Yaptığı her şey bilgisayarınızda olur. Bu sayfanın İngilizcesi depo kökündeki [PRIVACY.md](../PRIVACY.md) dosyasındadır.

## Kullanım bilgisi toplanmaz

- Uygulama kullanım bilgisi, istatistik ya da çökme raporu toplamaz; sizinle ya da bilgisayarınızla ilgili hiçbir şeyi bir yere göndermez.
- Hesap ve oturum açma yoktur.
- Günlük dosyaları ve çökme notları bilgisayarınızda kalır (bkz. [Verileriniz nerede durur](#verileriniz-nerede-durur)).

## Uygulama internete ne zaman bağlanır

- **Eklediğiniz takvimler.** Uygulama eklediğiniz her takvim adresini 15 dakikada bir ve siz istediğinizde okur. Bunun için Windows'un kendi HTTP istemcisini kullanır; sistemdeki vekil sunucu ve sertifika ayarları geçerlidir. Yalnızca `https://` adresleri kabul edilir (`webcal://` adresleri `https://` olarak okunur). Takvim sunucusuna başka bir şey gönderilmez.
- **Güncellemeler (planlanıyor).** Sonraki bir sürüm uygulama güncellemelerini denetleyecek. Bu denetimin ne gönderdiği, sürüm çıkmadan önce bu sayfaya yazılacak.

Uygulama başka bir ağ bağlantısı kurmaz.

## Uygulamanın bilgisayarınızda okudukları

Bu bilgiler yalnızca klavyede ve uygulamada göstermek için kullanılır. Belirtilenler dışında saklanmaz ve bilgisayarınızdan çıkmaz.

| Ne | Ne zaman | Neden |
|---|---|---|
| Çalan şarkı (ad, sanatçı, kapak, çalan uygulama), Windows medya oturumundan | Canlı mod ve "Çalan şarkı" kaynağı açıkken | Klavye ekranındaki şarkı kartı |
| İşlemci, ekran kartı ve bellek kullanımı (performans sayaçları) | Sistem göstergeleri açıkken ya da sayfası açıkken | Tuşlardaki gösterge çubukları |
| Hoparlör ses düzeyi ve sessizliği, mikrofon sessizliği (Windows ses sistemi) | Sistem göstergeleri açıkken ya da sayfası açıkken | Ses çubuğu ve sessizlik ışığı |
| Klavye ya da fareye son dokunuşunuzdan bu yana geçen süre | Hatırlatıcılar açıkken | Tekrarlayan hatırlatmalar siz yokken bekler |
| Windows klavye düzeniniz | Uygulama açılınca ve öne gelince | Tuş adlarının sizin dilinizde yazılması |
| Bağlı klavyeler (USB aygıtları, Bluetooth pil düzeyi) | Uygulama çalışırken | Klavyeyi bulmak ve yönetmek |
| Windows ve WebView2 sürümleri | Tanılama bilgisini kopyaladığınızda | Tanılama metni |

## Verileriniz nerede durur

Klasör adlarında uygulamanın kimliği geçer. **Ayarlar → Hakkında → Veri klasörü** ve **Günlük dosyaları** bu klasörleri açar.

- **Ayarlar ve kayıtlar** (`%APPDATA%\<uygulama kimliği>\`):
  - `calibration.json`, `clock.json`, `lighting.json`, `pomodoro.json`, `gauges.json`, `reminders.json`, `battery.json`;
  - `keymap.json` ve `macros.json`: uygulamanın klavyeye ne yazdığının tek kaydı (klavye bunları geri okutmaz);
  - `calendar.json`: takvimleriniz. Takvim adresleri parola gibidir; bu yüzden Windows kullanıcı hesabınıza özel olarak şifrelenir (Windows Veri Koruma). Başka bir kullanıcı ya da bilgisayar bunları okuyamaz;
  - `screen\`: görsel kütüphaneniz ve son canlı kart.
- **Günlük dosyaları** (`%LOCALAPPDATA%\<uygulama kimliği>\logs\`): her gün için bir dosya, son 7 gün, her biri en çok 4 MB; ayrıca kısa çökme notları. Yazılmadan önce web adresleri yalnızca sunucu adına indirilir ve Windows kullanıcı adınız klasör yollarından çıkarılır. Hatırlatma başlıkları ve toplantı adları hiç yazılmaz.
- **Arayüz tercihleri** (`%LOCALAPPDATA%\<uygulama kimliği>\EBWebView\`): tema, dil ve henüz kaydetmediğiniz düzenlemeler. Uygulamanın web görünümü tutar.

## Tanılama bilgisi

**Ayarlar → Hakkında → Tanılama bilgisi → Kopyala** panoya bir metin koyar: uygulama, Windows ve WebView2 sürümleri; klavye modeli, klavye yazılımı ve bağlantısı; ayar dosyalarının adları ve boyutları; günlüğün son 150 satırı (yukarıdaki gibi temizlenmiş). Bu metin hiçbir yere gönderilmez; bir iletiye yapıştırıp yapıştırmamaya siz karar verirsiniz.

## Dışa aktarma, içe aktarma ve silme

- **Dışa aktarma.** **Ayarlar → Yedekleme → Dışa aktar**, İndirilenler klasörünüze tek bir JSON dosyası kaydeder: ayarlarınız, tuş atama ve makro kayıtlarınız, hatırlatıcılarınız, takvim ayarlarınız, ışık tasarımınız ve görsel kütüphaneniz. Takvim adresleri yalnızca **Takvim adreslerini ekle** açıksa eklenir; o zaman dosyada açık yazılırlar, dosyayı kimseyle paylaşmayın.
- **İçe aktarma.** **Ayarlar → Yedekleme → Dosya seç**, bir şey değişmeden önce dosyada ne olduğunu ve neyin yerini alacağını gösterir. İçe aktarma klavyeye bir şey yazmaz: tuş atama, makrolar ve ışıklar kaydedilmemiş değişiklik olarak açılır, klavyeye kaydetmek size kalır.
- **Silme.** Uygulamayı kaldırın, ardından yukarıdaki klasörleri silin. Uygulama kapalıyken tek tek dosyaları da silebilirsiniz; uygulama bir sonraki açılışta varsayılan ayarlarla başlar.

## Klavyede kalanlar

Klavye bazı ayarları kendi belleğinde tutar. Bunlar uygulamayı kaldırdığınızda ya da klavyeyi başka bir bilgisayara taktığınızda da kalır:

- tuş atama ve makrolar;
- ışık efekti ya da tuş başına renk tasarımı;
- ekrandaki görsel;
- saat.

Klavyeyi başkasına vermeden önce bunları temizlemek için uygulamada tuş atamayı varsayılana döndürüp makroları silerek klavyeye kaydedin ve ekrana yansız bir görsel yükleyin.
