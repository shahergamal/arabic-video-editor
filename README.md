# محرر الفيديو (اسم مؤقت)

برنامج مونتاج لسطح المكتب (ويندوز وماك)، عربي أولاً: كتابة عربية صحيحة، خطوط عربية مفتوحة مضمّنة، ويعمل بالكامل بدون إنترنت.

## تنزيل البرنامج وتجربته

1. افتح صفحة **Releases** في المستودع، واختار آخر نسخة.
2. نزّل الملف المناسب لجهازك:
   - **ويندوز:** الملف اللي آخره `setup.exe`. لو ظهر تحذير "Windows protected your PC"، اضغط **More info** ثم **Run anyway** (البرنامج غير موقّع رقمياً لسه).
   - **ماك بشريحة Apple (M1 وما بعده):** الملف اللي فيه `aarch64.dmg`.
   - **ماك بمعالج Intel:** الملف اللي فيه `x64.dmg`.
   على الماك، لو ظهر "can't be opened" أو "damaged": اسحب البرنامج لـ Applications، ثم افتح Terminal واكتب:
   `xattr -cr "/Applications/محرر الفيديو.app"`
3. أثناء التشغيل، فوق المعاينة فيه رقم السرعة (إطار/ث) وعدد الإطارات الناقصة. لو الرقم أقل من سرعة الفيديو (30 مثلاً)، ابعتلنا صورة منه.

## اختصارات

| الاختصار | العمل |
| --- | --- |
| مسافة | تشغيل / إيقاف |
| S | قص المقطع عند المؤشر |
| Delete | حذف المختار (المقطع يُحذف ويُسحب الفراغ) |
| ← → | إطار واحد (مع Shift: ثانية) |
| Ctrl+Z / Ctrl+Shift+Z | تراجع / إعادة |
| Ctrl+S | حفظ المشروع |
| J / K / L | رجوع / وقوف / تقديم (اضغط L تاني للسرعة 2× و4×) |
| M | علامة عند المؤشر |
| ↑ ↓ | القفز للقطع أو العلامة اللي قبل/بعد |
| F | تجميد الصورة عند المؤشر |
| Ctrl+C / Ctrl+V / Ctrl+D | نسخ / لصق عند المؤشر / تكرار |
| Ctrl + ضغطة | اختيار أكتر من مقطع |
| Ctrl+G / Ctrl+Shift+G | تجميع المقاطع المختارة / فك المجموعة |
| Alt + سحب | وسط المقطع: تحريك المحتوى جوه (slip) · طرف المقطع: تحريك القطع بين مقطعين (roll) |

## النشر على السوشيال ميديا

زرار **نشر** فوق بيصدّر الفيديو بالمقاس الصح لفيسبوك وانستجرام وتيك توك (بوست، ريل، ستوري) مع الكابشن والهاشتاجات.
النشر المباشر من البرنامج محتاج ربط الحسابات مرة واحدة: الخطوات في [docs/publishing-ar.md](docs/publishing-ar.md).

## ضمانات مُختبرة تلقائياً مع كل تعديل

- النص العربي: حروف متصلة، لام ألف، تشكيل، من اليمين لليسار، إنجليزي وسط العربي، بكل خط مضمّن.
- التصدير يطابق المعاينة إطاراً بإطار، والصوت موجود والمدة صحيحة.
- الفيديو الأصلي لا يتغير أبداً.
- ملف المشروع لا يتلف: نسخة احتياطية دائماً، واسترجاع بعد الانهيار.
- إلغاء التصدير لا يترك ملفاً ناقصاً.

---

## For developers

```
crates/engine      Rust engine: project model, decoding (ffmpeg process), Arabic text
                   (cosmic-text / HarfBuzz rules), compositing, export
app/               Tauri 2 desktop app: React + TypeScript UI in app/src,
                   Rust commands in app/src-tauri
assets/fonts       Bundled open-license fonts + manifest.json (each with its license)
scripts/           fetch-fonts.mjs, fetch-ffmpeg.mjs, build-windows.ps1 (build + install on Windows)
```

Requirements: Rust (stable), Node 22, ffmpeg on PATH for tests.

```sh
cargo test -p engine                     # engine tests (need ffmpeg on PATH)
node scripts/fetch-ffmpeg.mjs            # bundled ffmpeg for the app
cd app && npm ci && npm run tauri dev    # run the app
```

Installers are built by `.github/workflows/release.yml` (Actions tab, "Run workflow").
Licensing notes: [docs/licenses.md](docs/licenses.md). Roadmap: [docs/roadmap.md](docs/roadmap.md).
