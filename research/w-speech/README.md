# W_speech Workshop

## Corpus v1: LibriSpeech (dev-clean)

**Source:** http://www.openslr.org/resources/12/dev-clean.tar.gz
**License:** CC BY 4.0 (Commercial-compatible with attribution)
**Attribution Requirement:** The product NOTICE must credit LibriSpeech (Panayotov et al.) per CC BY.

**Note:** The previous `voxserv` audio quality testing samples are legally unclear and remain restricted. They stay legitimate ONLY as TEST input (ACX/VAD harnesses — data not baked into the product); the training line is what it cannot cross.

### Resampling & HF Implication
**Resampling Method:** Rust `rubato` FastFixedIn (or equivalent high-quality sync interpolator) from 16kHz to 48kHz.
**LOUD FLAG ON HF IMPLICATION:** LibriSpeech is native 16kHz. This means absolutely nothing above 8kHz exists in the source audio! The 4-10kHz consonant band is only HALF covered. The resulting templates will have ZERO energy in the 8kHz-24kHz bins. This is a measured-world fact for v2 planning. We proceed anyway for v1.

### Fit Protocol (fit_protocol_v1)
- 128 mel bands (triangle)
- ALL frames
- 12 iterations
- Seed: 314159
- tau: 8
- K: 4

### File Hashes
```text
fead3e0c481f1c637fe6ec8edef42506c7c2d33a3fb7519bfdf5815b31150303  2277-149896-0000.flac
f9c3371b4d1c6e95e7c01298a26215c2be97bee22c5408885d8d29b6e0e4171c  2277-149896-0001.flac
ade2caad47934462292358ba49acf7f048ced0595615492b4b20866d118bf901  2277-149896-0002.flac
a99595b53e5d48524c2ba4903071f2498b05c25c509f0a78d60dc15ee46a4836  2277-149896-0003.flac
ed54e3c104353811fe907ea4042fca210ee4dc2db29067ee7d29b9a51c954a46  2277-149896-0004.flac
05013da97856ccce8ef62b3408643f144ae6b88d581731675fd1a70b03288849  2277-149896-0005.flac
3d63f01a67b524514d7bee6b966b58d4b6f18f6cfa1f92c847353b824593ee8c  2277-149896-0006.flac
addaf4f4264c7db2284e5517d1120f22c2b39c1bc58bbc46bc4f3625666891b4  2277-149896-0007.flac
fff4a9ac06e5a85348e1d12a498da564c7de250917e8733577b86f64cec7198c  2277-149896-0008.flac
91f02445f3a28cf518aaf8667368e1aba09997cca55208a40bddf1d2d1f2936e  2277-149896-0009.flac
13151a1366282c4b67916c6c266663ee52c84c88a0df264085066668b0ea0573  2277-149896-0010.flac
1c4046f47f1398e9cc33f3bb673c4802f8baa5c018172d5acbb2c625802bfb6a  2277-149896-0011.flac
33382f7634da27eeba65f1e8f945672c9ed3a9cf8c005e53e7f30cd53555f4f0  2277-149896-0012.flac
91d24f91ba4da2e1d10c302f4fc624ecda2bb73577e1dc1642d28613667a3dd8  2277-149896-0013.flac
d626ac1858e0a373bcd84d81a631ebe57ff7fea3fc83beae15bcc613d6e3c179  2277-149896-0015.flac
35535a9a29d599998fb7addaa884a9272da4a80f2e92487008b4bcb66e2b7dfa  2277-149896-0016.flac
b92316c9aa49903883f601ffcbfe67a7647569be7454aa59c10467228f4cefca  2277-149896-0017.flac
6a52810a9b4656d86ef50cda41415f6d242e43939e0c501b9a02c63b9a869461  2277-149896-0018.flac
5eb7bd90711107c2910b74d43f164ef66b162de88106c6ef610c2a8fe1139bf3  2277-149896-0019.flac
c6bcb66167d4562cf90e876dead087625d1ac5058656bd053432266f54933044  2277-149896-0021.flac
4ae7a40d4cd554616da367bc0464a56caee0658ea5af7db328ee03ae1a5964c4  2277-149896-0023.flac
d67e8a52aaee3822c461bb21eda0cbbb61201a67c03700f6f44d4814bbceeaa5  2277-149896-0024.flac
3909a6c5ae93112518681fcbf0a74df43615a4f88bd6d5cd788775255296e3f6  2277-149896-0026.flac
a4831606b567c1a81940e11ccafa0ae14767ab43dde5ce7dbba56d54500bb13e  2277-149896-0027.flac
1329424372f495a29b6f58485b377bbda860056606425f15c2f402afdb1d98af  2277-149896-0028.flac
c6344911546ea7bfc243fba7e723f6d20bf89328629080100459924a81acdd32  2277-149896-0030.flac
74a08b4880512f6e43f5f2e45eaf2b6078184f46a89f6b862a2eb1c5f25ebbcd  2277-149896-0031.flac
56b640a079f27f9473478a20f068b8bd8af3afa7030d7a07cbbc7b3f19211689  2277-149896-0032.flac
23009ec1ed5c704bb4b1c0bc75953121f38b2a2136cf8380f98e512517427473  2277-149896-0033.flac
aa063190b3125aa8400908460213bafb71226b4980bef44b81c7dd2658cebacb  2277-149896-0034.flac
```
