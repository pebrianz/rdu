use ratatui::style::{Color, palette::tailwind};

pub struct TableColors {
    pub buffer_bg: Color,
    pub header_bg: Color,
    pub header_fg: Color,
    pub row_fg: Color,
    pub selected_row_style_fg: Color,
    pub selected_row_style_bg: Color,
    pub selected_column_style_fg: Color,
    pub selected_cell_style_fg: Color,
    pub footer_border_color: Color,
}

impl TableColors {
    pub const fn new() -> Self {
        Self {
            buffer_bg: tailwind::SLATE.c950,
            header_bg: tailwind::GREEN.c700,
            header_fg: tailwind::SLATE.c200,
            row_fg: tailwind::SLATE.c200,
            selected_row_style_fg: Color::White,
            selected_row_style_bg: tailwind::TEAL.c900,
            selected_column_style_fg: tailwind::RED.c400,
            selected_cell_style_fg: tailwind::RED.c600,
            footer_border_color: tailwind::RED.c400,
        }
    }
}
