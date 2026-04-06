use std::{
    io::{self, Write},
    rc::Rc,
};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    gfx::{Color, Point, Rect, Size},
    input::Key,
    ui::navigation::{Navigation, NavigationAction},
    utils::log,
};

use super::{Cell, Grapheme, Painter};

pub struct Renderer {
    nav: Navigation,
    cells: Vec<(Cell, Cell)>,
    painter: Painter,
    size: Size,
    page_background: Color,
    text_masks: Vec<TextMask>,
}

#[derive(Clone, Copy)]
struct TextMask {
    col: usize,
    row: usize,
    width: usize,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            nav: Navigation::new(),
            cells: Vec::with_capacity(0),
            painter: Painter::new(),
            size: Size::new(0, 0),
            page_background: Color::black(),
            text_masks: Vec::new(),
        }
    }

    pub fn enable_true_color(&mut self) {
        self.painter.set_true_color(true)
    }

    pub fn keypress(&mut self, key: &Key) -> io::Result<NavigationAction> {
        let action = self.nav.keypress(key);

        Ok(action)
    }
    pub fn mouse_up(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_up(origin);

        Ok(action)
    }
    pub fn mouse_down(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_down(origin);

        Ok(action)
    }
    pub fn mouse_move(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_move(origin);

        Ok(action)
    }

    pub fn push_nav(&mut self, url: &str, can_go_back: bool, can_go_forward: bool) {
        self.nav.push(url, can_go_back, can_go_forward)
    }

    pub fn get_size(&self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        self.nav.set_size(size);
        self.size = size;

        let mut x = 0;
        let mut y = 0;
        let bound = size.width - 1;
        let cells = (size.width + size.width * size.height) as usize;

        self.cells.clear();
        self.cells.resize_with(cells, || {
            let cell = (Cell::new(x, y), Cell::new(x, y));

            if x < bound {
                x += 1;
            } else {
                x = 0;
                y += 1;
            }

            cell
        });
    }

    pub fn render(&mut self) -> io::Result<()> {
        let size = self.size;

        for (origin, element) in self.nav.render(size) {
            self.fill_rect(
                Rect::new(origin.x, origin.y, element.text.width() as u32, 1),
                element.background,
            );
            self.draw_text(
                &element.text,
                origin * (2, 1),
                Size::splat(0),
                element.foreground,
            );
        }

        self.normalize_background_noise();

        self.painter.begin()?;

        for (previous, current) in self.cells.iter_mut() {
            if current == previous {
                continue;
            }

            previous.quadrant = current.quadrant;
            previous.grapheme = current.grapheme.clone();

            self.painter.paint(current)?;
        }

        self.painter.end(self.nav.cursor())?;

        Ok(())
    }

    /// Draw the background from a pixel array encoded in RGBA8888
    pub fn draw_background(&mut self, pixels: &[u8], pixels_size: Size, rect: Rect) {
        let viewport = self.size.cast::<usize>();

        if pixels.len() < viewport.width * viewport.height * 8 * 4 {
            log::debug!(
                "unexpected size, actual: {}, expected: {}",
                pixels.len(),
                viewport.width * viewport.height * 8 * 4
            );
            return;
        }

        let origin = rect.origin.cast::<f32>().max(0.0) / (2.0, 4.0);
        let size = rect.size.cast::<f32>().max(0.0) / (2.0, 4.0);
        let top = (origin.y.floor() as usize).min(viewport.height);
        let left = (origin.x.floor() as usize).min(viewport.width);
        let right = ((origin.x + size.width).ceil() as usize)
            .min(viewport.width)
            .max(left);
        let bottom = ((origin.y + size.height).ceil() as usize)
            .min(viewport.height)
            .max(top);
        let row_length = pixels_size.width as usize;
        let pixel = |x, y| {
            Color::new(
                pixels[((x + y * row_length) * 4 + 2) as usize],
                pixels[((x + y * row_length) * 4 + 1) as usize],
                pixels[((x + y * row_length) * 4 + 0) as usize],
            )
        };
        let pair = |x, y| pixel(x, y).avg_with(pixel(x, y + 1));

        for y in top..bottom {
            let index = (y + 1) * viewport.width;
            let start = index + left;
            let end = index + right;
            let (mut x, y) = (left * 2, y * 4);

            for (_, cell) in &mut self.cells[start..end] {
                cell.quadrant = (
                    pair(x + 0, y + 0),
                    pair(x + 1, y + 0),
                    pair(x + 1, y + 2),
                    pair(x + 0, y + 2),
                );

                x += 2;
            }
        }

        for mask in self.text_masks.clone() {
            self.scrub_text_background(mask.col, mask.row, mask.width);
        }
    }

    pub fn clear_text(&mut self) {
        self.text_masks.clear();
        for (_, cell) in self.cells.iter_mut() {
            cell.grapheme = None
        }
    }

    pub fn set_title(&self, title: &str) -> io::Result<()> {
        let mut stdout = io::stdout();

        write!(stdout, "\x1b]0;{title}\x07")?;
        write!(stdout, "\x1b]1;{title}\x07")?;
        write!(stdout, "\x1b]2;{title}\x07")?;

        stdout.flush()
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let viewport = self.size.cast::<usize>();
        let origin = rect.origin.cast::<f32>();
        let size = rect.size.cast::<f32>();
        let left = ((origin.x / 2.0).floor().max(0.0) as usize).min(viewport.width);
        let right = (((origin.x + size.width) / 2.0).floor().max(0.0) as usize).min(viewport.width);
        let top = (((origin.y / 4.0) + 1.0).floor().max(0.0) as usize).min(viewport.height);
        let bottom =
            ((((origin.y + size.height) / 4.0) + 1.0).floor().max(0.0) as usize).min(viewport.height);

        if left >= right || top >= bottom {
            return;
        }

        if left == 0 && top == 1 && right == viewport.width && bottom == viewport.height {
            self.page_background = color;
        }

        self.draw(
            Rect {
                origin: Point::new(left, top).cast(),
                size: Size::new(right.saturating_sub(left), bottom.saturating_sub(top)).cast(),
            },
            |cell| {
            cell.grapheme = None;
            cell.quadrant = (color, color, color, color);
            },
        )
    }

    pub fn draw<F>(&mut self, bounds: Rect, mut draw: F)
    where
        F: FnMut(&mut Cell),
    {
        let origin = bounds.origin.cast::<usize>();
        let size = bounds.size.cast::<usize>();
        let viewport_width = self.size.width as usize;
        let top = origin.y;
        let bottom = top + size.height;

        // Iterate over each row
        for y in top..bottom {
            let left = y * viewport_width + origin.x;
            let right = left + size.width;

            for (_, current) in self.cells[left..right].iter_mut() {
                draw(current)
            }
        }
    }

    /// Render some text into the terminal output
    pub fn draw_text(&mut self, string: &str, origin: Point, size: Size, color: Color) {
        // Get an iterator starting at the text origin
        let len = self.cells.len();
        let viewport = &self.size.cast::<usize>();

        if size.width > 2 && size.height > 2 {
            let origin = (origin.cast::<f32>() / (2.0, 4.0) + (0.0, 1.0)).round();
            let size = (size.cast::<f32>() / (2.0, 4.0)).round();
            let left = (origin.x.max(0.0) as usize).min(viewport.width);
            let right = ((origin.x + size.width).max(0.0) as usize).min(viewport.width);
            let top = (origin.y.max(0.0) as usize).min(viewport.height);
            let bottom = ((origin.y + size.height).max(0.0) as usize).min(viewport.height);

            for y in top..bottom {
                let index = y * viewport.width;
                let start = index + left;
                let end = index + right;

                for (_, cell) in self.cells[start..end].iter_mut() {
                    cell.grapheme = None
                }
            }
        } else {
            let text_width = string.width();
            let text_row = ((origin.y + 1).max(0) / 4) as usize;
            let text_col = (origin.x.max(0) / 2) as usize;

            self.text_masks.push(TextMask {
                col: text_col,
                row: text_row,
                width: text_width,
            });
            self.scrub_text_background(text_col, text_row, text_width);

            // Compute the buffer index based on the position
            let index = origin.x / 2 + (origin.y + 1) / 4 * (viewport.width as i32);
            let mut iter = self.cells[len.min(index as usize)..].iter_mut();

            // Get every Unicode grapheme in the input string
            for grapheme in UnicodeSegmentation::graphemes(string, true) {
                let width = grapheme.width();

                for index in 0..width {
                    // Get the next terminal cell at the given position
                    match iter.next() {
                        // Stop if we're at the end of the buffer
                        None => return,
                        // Set the cell to the current grapheme
                        Some((_, cell)) => {
                            let next = Grapheme {
                                // Create a new shared reference to the text
                                color,
                                index,
                                width,
                                // Export the set of unicode code points for this graphene into an UTF-8 string
                                char: grapheme.to_string(),
                            };

                            if match cell.grapheme {
                                None => true,
                                Some(ref previous) => {
                                    previous.color != next.color || previous.char != next.char
                                }
                            } {
                                cell.grapheme = Some(Rc::new(next));
                            }

                        }
                    }
                }
            }
        }
    }

    fn scrub_text_background(&mut self, col: usize, row: usize, text_width: usize) {
        let viewport = self.size.cast::<usize>();
        if viewport.width == 0 || viewport.height == 0 {
            return;
        }

        let left = col.saturating_sub(2).min(viewport.width);
        let right = col
            .saturating_add(text_width)
            .saturating_add(8)
            .min(viewport.width);
        let top = row.min(viewport.height.saturating_sub(1));
        let bottom = row.saturating_add(2).min(viewport.height);

        if left >= right || top >= bottom {
            return;
        }

        let background = self
            .sample_text_background(left, right, top, bottom)
            .unwrap_or(self.page_background);

        self.draw(
            Rect {
                origin: Point::new(left, top).cast(),
                size: Size::new(right - left, bottom - top).cast(),
            },
            |cell| {
                cell.quadrant = (background, background, background, background);
            },
        );
    }

    fn sample_text_background(
        &self,
        left: usize,
        right: usize,
        top: usize,
        bottom: usize,
    ) -> Option<Color> {
        let viewport_width = self.size.width as usize;
        let mut samples = Vec::new();

        for y in top..bottom {
            if left > 1 {
                samples.push(self.cell_color(left - 2, y, viewport_width));
            }
            if right < viewport_width {
                samples.push(self.cell_color(right, y, viewport_width));
            }
        }

        if top > 0 {
            for x in left..right {
                samples.push(self.cell_color(x, top - 1, viewport_width));
            }
        }

        if bottom < self.size.height as usize {
            for x in left..right {
                samples.push(self.cell_color(x, bottom, viewport_width));
            }
        }

        if samples.is_empty() {
            None
        } else {
            samples
                .into_iter()
                .max_by_key(|color| color.r as u16 + color.g as u16 + color.b as u16)
        }
    }

    fn cell_color(&self, x: usize, y: usize, viewport_width: usize) -> Color {
        let index = y * viewport_width + x;
        let quadrant = self.cells[index].1.quadrant;

        quadrant
            .0
            .avg_with(quadrant.1)
            .avg_with(quadrant.2)
            .avg_with(quadrant.3)
    }

    fn normalize_background_noise(&mut self) {
        let page_background = self.page_background;

        for (_, cell) in self.cells.iter_mut() {
            if cell.grapheme.is_some() {
                continue;
            }

            if Self::matches_background(cell.quadrant, page_background) {
                cell.quadrant = (
                    page_background,
                    page_background,
                    page_background,
                    page_background,
                );
            }
        }
    }

    fn matches_background(quadrant: (Color, Color, Color, Color), background: Color) -> bool {
        let threshold = 28;

        Self::color_distance(quadrant.0, background) <= threshold
            && Self::color_distance(quadrant.1, background) <= threshold
            && Self::color_distance(quadrant.2, background) <= threshold
            && Self::color_distance(quadrant.3, background) <= threshold
    }

    fn color_distance(left: Color, right: Color) -> u8 {
        let dr = left.r.abs_diff(right.r);
        let dg = left.g.abs_diff(right.g);
        let db = left.b.abs_diff(right.b);

        dr.max(dg).max(db)
    }
}
