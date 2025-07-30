# Changelog

## [0.4.0](https://github.com/mbhall88/nohuman/compare/0.3.0...0.4.0) (2025-07-30)


### Features

* add Kraken2 report option with aggregate counts/clade ([#14](https://github.com/mbhall88/nohuman/issues/14)) ([100b98b](https://github.com/mbhall88/nohuman/commit/100b98b1ef075e7a600e38d61cb4d7a05f9b3583))

## [0.3.0](https://github.com/mbhall88/nohuman/compare/0.2.1...0.3.0) (2024-10-01)


### Features

* add confidence threshold option `--conf` ([419ede9](https://github.com/mbhall88/nohuman/commit/419ede9cf5997692409106e294eb168ddc5427b7))
* add kraken2 classification output file option `-k` ([a4146bb](https://github.com/mbhall88/nohuman/commit/a4146bb04c8dbef071366cec3be40118bc38a3d1))

## [0.2.1](https://github.com/mbhall88/nohuman/compare/0.2.0...0.2.1) (2024-09-23)


### Bug Fixes

* **deps:** remove openssl dep and downgrade lzma ([1f2da59](https://github.com/mbhall88/nohuman/commit/1f2da592f7423da966fbfa6954e06b5a41c8eb01))

## [0.2.0](https://github.com/mbhall88/nohuman/compare/0.1.1...0.2.0) (2024-09-23)


### Features

* add bzip2 compression ([05e8b91](https://github.com/mbhall88/nohuman/commit/05e8b9134a4316a9660cdf64c5783a0029616e90))
* add flag to keep human reads instead ([50a5ccb](https://github.com/mbhall88/nohuman/commit/50a5ccb7f93ca299a49aa8e9f7e2bf3e6f4de76b))
* add gzip compression of output ([79bbbf4](https://github.com/mbhall88/nohuman/commit/79bbbf4e299213e29379c8465e560bb81e0f73e6))
* add kraken2 stats to log ([c8fe418](https://github.com/mbhall88/nohuman/commit/c8fe418d46ced5c3c1f4343dc96d6548c73125fe))
* add xz compression of output ([00d5ffa](https://github.com/mbhall88/nohuman/commit/00d5ffa3d7aed92a387c6c71a610bcbc6849b6db))
* add zstd compression of output ([5de5332](https://github.com/mbhall88/nohuman/commit/5de53325c52b32663d9f0c5c13e15844076502a7))

## [0.1.1](https://github.com/mbhall88/nohuman/compare/0.1.0...0.1.1) (2024-07-22)


### Bug Fixes

* IMPORTANT!! previous versions were emitting human reads [see [#2](https://github.com/mbhall88/nohuman/issues/2)] ([319eaed](https://github.com/mbhall88/nohuman/commit/319eaedfbafa5a762a0aa5bdedafc2fcbe68bfc9))
* more robust validation of db directory ([0377cd6](https://github.com/mbhall88/nohuman/commit/0377cd612e190f651c276145531eb285cf7927ba))

## 0.1.0 (2023-12-14)


### Features

* add database download functionality ([0ebc674](https://github.com/mbhall88/nohuman/commit/0ebc674d789045e529de8fa3eaa9a63efb1175cd))
* add progress bar ([3c29372](https://github.com/mbhall88/nohuman/commit/3c2937216bf1a8bc5834b6abbf02d52c72ff8ef0))
* check dependencies option ([64353a3](https://github.com/mbhall88/nohuman/commit/64353a3815b686d3b40abc7238ff6684d3dbf4ed))
* initial commit ([3752150](https://github.com/mbhall88/nohuman/commit/37521500aaf87377f961f8f84a0882a4b4f8e5af))
* run kraken ([90a6e14](https://github.com/mbhall88/nohuman/commit/90a6e1494ba492b801927db57f4bb95b9ac896b5))


### Bug Fixes

* check input path exists when parsing cli ([1ce7150](https://github.com/mbhall88/nohuman/commit/1ce7150a89fbb24971a8d83dc4b1c8e6bf9665fa))
* correct db path ([9ea852f](https://github.com/mbhall88/nohuman/commit/9ea852fb43a562c859d2f364ef4b39c02731f775))
* don't load whole database into memory when downloading ([3e9ea23](https://github.com/mbhall88/nohuman/commit/3e9ea232a8e9f32566781592f2b65e2c6b7e1328))
* dont check if db exists on --check ([16bf181](https://github.com/mbhall88/nohuman/commit/16bf1816e027fd23ab91dec4903818a6f70a70f7))
* exit gracefully after download if no input ([189820d](https://github.com/mbhall88/nohuman/commit/189820db680910d53e19d95b59803e977c51686c))
* ignore gz suffix when naming output ([c558ce7](https://github.com/mbhall88/nohuman/commit/c558ce7b7863ae8043a7d46607dee0ef9b9db52a))
* make is_executable more portable ([c228686](https://github.com/mbhall88/nohuman/commit/c228686df9024f9a4475fe5f596a7dbfe56c4bc6))
