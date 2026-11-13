# Free CC0 Materials Library / Библиотека свободных материалов (CC0)

Этот каталог предназначен для материалов и текстур проекта. Все материалы в этой системе могут свободно использоваться разработчиками как для кубических вокселей, так и для 3D-моделей (персонажей, деревьев, техники, оружия в стиле *Unturned* или *7 Days to Die*).

## Бесплатные источники профессиональных CC0 материалов:
1. **[Poly Haven](https://polyhaven.com/textures)** — 100% бесплатные (Public Domain) фотореалистичные PBR материалы со всеми картами (камень, дерево, металл, земля).
2. **[AmbientCG](https://ambientcg.com/)** — более 1,500 бесплатных PBR материалов в высоком качестве.
3. **[Kenney.nl](https://kenney.nl/assets)** — сотни бесплатных CC0 стилизованных 3D-моделей и текстур для стилизованных игр (Unturned / Low-Poly).

## Формат описания материала (.toml):
```toml
name = "custom:mossy_cobble"
shading_model = "StandardPbr" # StandardPbr, Stylized, Foliage, Emissive
alpha_mode = "Opaque"         # Opaque, Cutout, Blend
texture_id = 5                # Индекс текстуры в атласе
roughness = 0.88              # 0.0 (глянец) .. 1.0 (матовый)
metallic = 0.0                # 0.0 (диэлектрик) .. 1.0 (металл)
base_color = [0.85, 0.95, 0.80, 1.0] # RGBA тинт
subsurface = 0.0              # Просвечивание листвы / SSS
emission = [0.0, 0.0, 0.0]    # Свечение
```
