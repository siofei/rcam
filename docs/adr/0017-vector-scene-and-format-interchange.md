# ADR 0017：VectorScene 与多格式交换

2026-09-19；Post-V1 架构预留，不扩大 V1 96 个用例或当前能力表。
Gerber → gerber-parser/gerber-io → Manufacturing Model (SemanticDocument)，不强制经过 VectorScene。
DXF/SVG/HP-GL(PLT) → source parser → VectorScene → explicit Manufacturing Conversion → Manufacturing Model。
VectorScene 仅通用矢量几何、层、变换；不代替 Gerber 的 Dark/Clear、Aperture、Region、曝光顺序和兼容语义。
单位、默认线宽、层/笔/颜色映射、ClosedPath stroke/fill/Region、Bezier/Spline 拟合误差须显式转换并验证；歧义拒绝或要求选择。
矢量导出读取语义/几何模型，禁止从 GPU Mesh、像素、显示细分或当前缩放路径反推 Gerber/DXF/SVG/PDF。PNG 可用受控离屏 raster adapter。
本轮不实现 DXF/SVG/PLT 导入导出，不增加依赖、公共 API、空菜单或已支持声明。专门 F 阶段前先更新需求、验收、ADR，并审查依赖许可和输入安全。
ADR 0016 保留给 S2-A.3 GUI；旧草案 0015 引用统一迁移至本编号。
