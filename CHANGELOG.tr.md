# Sürüm notları

Keymotion'daki önemli değişiklikler burada listelenir. İngilizcesi: [CHANGELOG.md](CHANGELOG.md).

## [Yayımlanmamış]

## [0.1.0] - 2026-11-12

İlk sürüm. Windows'ta AJAZZ AK980 PRO'yu destekler.

### Eklenenler

- **Bağlantı:** Klavye USB kablo, 2.4 GHz alıcı ve Bluetooth bağlantısında
  tanınır; takıldığı ya da çıkarıldığı fark edilir. Bağlantı sayfası her modda
  neler yapılabildiğini gösterir; ayarlar için USB kablo gerekir. Bluetooth
  bağlantısında pil düzeyi gösterilir.
- **Genel bakış:** Sayfa genişliğince, ölçülerek çizilmiş 97 tuşlu düzen ve her
  tuşun üzerindeki harfler; ardından klavye ekranında şu an görünen görsel ve
  her araç için bir kutucuk: ışık, odak zamanlayıcısı (buradan
  başlatılabilir), sıradaki hatırlatma, sıradaki toplantı, canlı çubuklarıyla
  sistem göstergeleri ve profil. Ayrıca bağlantı modu, pil ve klavye saati;
  saat, klavye bağlanınca kendiliğinden ayarlanır.
- **Işıklar:** Renk, parlaklık, hız ve yön ayarıyla 19 hazır efekt; klavyenin
  renklerinin canlı yansıması; fırça ve seçim araçlarıyla tuş başına renk,
  hazır desenler, canlı önizleme ve klavyeye kaydetme.
- **Renk kalibrasyonu:** Klavyede gösterilen test desenleriyle gama ve beyaz
  dengesi ayarı.
- **Tuş atama:** Temel katman ve Fn katmanı. Bir tuşa başka bir tuş, kısayol,
  medya tuşu, fare düğmesi ya da kaydırma atanabilir veya tuş devre dışı
  bırakılabilir; bir katman fabrika düzenine sıfırlanabilir.
- **Makrolar:** Tuşlara basılarak kaydedilir, adım adım düzenlenir (bekleme
  süreleri, sıra, fare tıklamaları) ve bir oynatma biçimiyle tuşlara atanır;
  klavye belleğinde ne kadar yer kaldığı görünür.
- **Ekran stüdyosu:** 240×135 ekran için görseller, GIF'ler ve videolar
  (kırpma, süre), yazı kartları ve slayt gösterileri; yüklemeden önce
  önizleme, kütüphane ve fabrika animasyonunu geri yükleme. "Işıkları
  eşleştir", görseldeki renklerden tuş renkleri tasarlar.
- **Canlı ekran:** Kendiliğinden güncellenen kartlar (çalan şarkı, odak
  zamanlayıcısının aşamaları, yaklaşan toplantılar); günlük yükleme sınırı,
  ekranda neyin görüneceğini belirleyen açık bir sıra ve ardından kendi
  görselinize dönüş. Odak zamanlayıcısı ve şarkı, son yükleme hakkını
  hatırlatmalara ve toplantılara bırakır. Sınırı bekleyen bir kart, klavyede
  o an görünenden ayrı gösterilir ve beklediği belirtilir; "Gönderme" o kartı
  sıradan çıkarır.
- **Takvim:** Google, Outlook ve iCloud takvimleri paylaşım adresleriyle
  eklenir (yalnızca okunur); toplantıdan önce ekranda bir kart çıkar,
  başlamasına 1 dk kala bir tuş yanıp söner.
- **Hatırlatıcılar:** Bir kez, her gün ya da çalışma saatlerinde aralıklarla;
  tuşlarda bir renk dalgası, isteğe bağlı olarak ekranda bir kart ve ses.
  "Dene" kartı da gösterir; sayfa, bir kartın neden gösterilmediğini belirtir
  (yükleme sınırı, canlı ekranın kapalı olması, USB kablonun takılı olmaması,
  toplantı kartı).
- **Sistem göstergeleri:** İşlemci, ekran kartı ve bellek yükü tuş
  sıralarında çubuk olarak, ses düzeyi işlev tuşlarında, mikrofonun sessize
  alındığı seçtiğiniz bir tuşta gösterilir.
- **Odak zamanlayıcısı:** Odak ve mola bloklarını istediğiniz uzunlukta ve
  istediğiniz sırayla dizerek kendi sıranızı kurarsınız (blokların yerini
  sürükleyerek değiştirebilirsiniz); sıra bir kez, birkaç kez ya da siz
  durdurana kadar çalışır. Başlangıçta "25 / 5 klasik" seçilidir; eski
  ayarlar, karşılık geldikleri sıraya dönüştürülür. Bütün klavye bloğun
  rengiyle dolar ve süre azaldıkça boşalır; her blok için ekranda bir kart
  çıkar.
- **Kurulum:** Windows için İngilizce ve Türkçe kurulum programı; uygulamayı
  yalnızca sizin hesabınıza kurar, Keymotion simgesini ve sade bir karşılama
  metnini gösterir, yönetici izni gerektirmez. Kaldırırken "Ayarlarımı ve
  kayıtlarımı da sil" işaretlenmezse ayarlarınız korunur; kaldırma işlemi
  Windows'la başlatma kaydını ve bildirimler için kaydedilen adı da siler.
- **Sorun bildirme:** Ayarlar › Hakkında › Geri bildirim, beklenmeyen bir
  hatadan sonra çıkan göze batmayan uyarı ve uygulama çöktükten sonra bir kez
  çıkan uyarı, sorun bildirme penceresini açar; pencere gönderilecek bilgileri
  önce gösterir. "GitHub'da bildir" bilgileri kopyalar ve projenin sorun
  formunu varsayılan tarayıcıda açar; uygulama kendiliğinden hiçbir şey
  göndermez. Henüz desteklenmeyen bir SONiX ya da AJAZZ klavye için "Destek
  iste" çıkar; isteğe klavyenin Windows'tan okunan USB kimlikleri ve HID
  tanımları eklenir (klavyeye hiçbir şey yazılmaz).
- **Keymotion animasyonu:** Klavye ekranı için Keymotion'ın kendi animasyonu,
  kütüphanede hazır: logodaki "K" çizgi çizgi çizilir, ardından Keymotion
  yazısı görünür (37 kare, yüklemesi yaklaşık 1,5 dk). Fabrika animasyonunun
  yedeği yoksa önerilir.
- **Saatin yanındaki simge:** Pencereyi kapattığınızda Keymotion saatin
  yanında çalışmaya devam eder; böylece odak zamanlayıcısı, hatırlatıcılar,
  takvim kartları, canlı ekran ve tuşlardaki göstergeler durmaz. Simgenin
  menüsü klavyeyi ve pilini gösterir, odak zamanlayıcısını yönetir (aşama ve
  kalan dakika, duraklatma, sonraki aşamaya geçme, durdurma), sıradaki
  hatırlatmayı gösterir, hatırlatıcıları ve tuşlardaki göstergeleri açıp
  kapatır. Menüden bir profilin yükleme onayı da açılır ve uygulamadan
  çıkılır. Bu davranış Ayarlar › Kapatma ve açılış bölümünden kapatılabilir;
  aynı yerden Keymotion'ın Windows açılınca penceresiz başlaması da
  seçilebilir. Uygulama
  açıldıktan sonra pencere ilk kez kapatıldığında, Keymotion'ın hâlâ
  çalıştığını bildiren bir uyarı görünür.
- **Güncellemeler:** Ayarlar › Hakkında bölümünden GitHub'da yeni sürüm olup
  olmadığı denetlenir; uygulama da açıldığında günde bir kez kendiliğinden
  bakar (bu kapatılabilir). Yeni sürüm bulununca sayfanın üstünde bir şerit
  görünür; "Kur ve yeniden başlat"ı seçmediğiniz sürece hiçbir şey kurulmaz.
  İndirilen dosya uygulamanın kendi anahtarıyla doğrulanır; kurulum, klavyeye
  süren bir yazma işleminin bitmesini bekler.
- **Açılış animasyonu:** Pencere açılırken logodaki "K" çizgi çizgi çizilir ve
  Keymotion adı açılır, ardından logo soldaki menüde kendi yerine geçer
  (uygulama hazırlanırken yaklaşık bir saniye). Keymotion Windows açılınca
  penceresiz başladığında ya da saatin yanındaki simgeden yeniden açıldığında
  oynatılmaz; bir tıklama ya da tuşa basma animasyonu bitirir; Windows'ta
  animasyon efektleri kapalıysa yalnızca solarak kaybolur.
- Sayfadan ayrılırken ya da uygulama kapatılırken kaydedilmemiş değişiklikler
  için koruma.
- Türkçe ve İngilizce arayüz, açık ve koyu tema.
- Proje: MIT/Apache-2.0 lisansı, NOTICE (resmî olmayan proje, kullanım riski
  size aittir), üçüncü taraf lisans bildirimleri ve bozuk karakterler, üretici
  dosyaları, sürümler ve bağımlılıklar için otomatik denetimler.
- **Belgeler:** cebersaban44-rgb.github.io/keymotion adresinde Türkçe ve
  İngilizce kullanım kılavuzu: uygulamanın her sayfası, bir şey ters giderse ne
  yapılacağı, desteklenen ve sırada olabilecek klavyeler, geliştiriciler için
  sayfalar. Ayarlar › Hakkında'dan açılır.

[Yayımlanmamış]: https://github.com/cebersaban44-rgb/keymotion/compare/v0.1.0...develop
[0.1.0]: https://github.com/cebersaban44-rgb/keymotion/releases/tag/v0.1.0
