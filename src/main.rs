#[macro_use]
extern crate lazy_static;

use fs_extra::dir::{copy, CopyOptions};
use rsass::{compile_scss_path, output};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use tera::Tera;

#[derive(Debug, Deserialize, Serialize)]
enum Row {
    Link {
        ico: String,
        lbl: String,
        url: String,
    },
    Separator,
}

#[derive(Debug, Deserialize, Serialize)]
struct Category {
    name: String,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Column {
    categories: Vec<Category>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Startpage {
    name: String,
    columns: Vec<Column>,
}

#[derive(Debug, Serialize)]
struct NavItem {
    name: String,
    path: String,
}

/// Startpages in the order they are defined in the configuration.
type Navigation = Vec<NavItem>;

static OUT_DIR: &str = "_site";
static CONFIGURATION: &str = "content/startpages.yaml";

// ---------------------------------------------------------------------------
// Geometry
//
// The page is a drawn map, not a flow layout, so the generator has to know
// where every word lands before the browser sees it. Monospace makes that
// exact: a label is `characters * advance` wide, full stop.
//
// The tree has four levels. The title is the core; a limb reaches out to every
// category; each category fans to the buds that stand for its separator-
// delimited groups; each bud fans to its links. Categories are shared between a
// left and a right field and centred on the core, so the whole thing grows
// outward from the middle.
//
// EVERY CONSTANT HERE HAS TO AGREE WITH sass/styles.scss. Change a font size on
// one side only and the veins stop meeting the words.
// ---------------------------------------------------------------------------

/// Advance width of one character at the leaf font size.
const CH: f32 = 8.4;
/// Advance width of one character in a category name, tracking included.
const NAME_CH: f32 = 10.4;
/// Advance width of one character in the title.
const ROOT_CH: f32 = 16.7;
/// Advance width of one character in a link to another map.
const OTHER_CH: f32 = 8.6;

/// Vertical pitch between leaves.
const ROW: f32 = 25.0;
/// Space between one bud's leaves and the next bud's.
const GROUP_GAP: f32 = 25.0;
/// Room the icon and its gap occupy beside a label.
const ICON: f32 = 24.0;
/// Bud to the leaves hanging off it.
const BUD_DX: f32 = 26.0;
/// Trailing space after a fan, before the next fan starts.
const COL_PAD: f32 = 46.0;
/// Category node to its name, and its name to the first fan.
const NAME_LEAD: f32 = 17.0;
const NAME_TRAIL: f32 = 28.0;
/// Smallest clearance between a category and whatever is packed to its right.
/// The layout opens this up until the map fills the screen.
const HGAP: f32 = 58.0;
/// Smallest clearance between a category and whatever is packed under it.
const VGAP: f32 = 38.0;
/// The screen the map is drawn to fill. Smaller windows pan; on a larger one
/// the map simply sits in the middle of it.
const TARGET_W: f32 = 1840.0;
const TARGET_H: f32 = 1000.0;
/// Resolution of the packing skyline.
const STEP: f32 = 4.0;
/// How far a limb reaches before it arrives at the first category.
const LIMB_REACH: f32 = 96.0;

const MARGIN: f32 = 48.0;
/// Strip at the top holding the links to the other maps.
const TOP_BAR: f32 = 68.0;

/// How many tones the stylesheet defines.
const TONES: usize = 9;

/// Rounded to whole pixels - enough for the renderer, short enough that the
/// generated markup stays readable.
fn px(value: f32) -> f32 {
    value.round()
}

/// A deterministic wobble. Everything drawn here carries a little drift, so the
/// map reads as something grown rather than something plotted.
fn drift(k: f32, amount: f32) -> f32 {
    (k * 1.9).sin() * amount
}

/// Catmull-Rom through the given points, as a cubic bezier path.
fn thread(points: &[(f32, f32)]) -> String {
    if points.len() < 2 {
        return String::new();
    }

    let mut d = format!("M{:.1},{:.1}", points[0].0, points[0].1);

    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];

        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);

        d.push_str(&format!(
            "C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            c1.0, c1.1, c2.0, c2.1, p2.0, p2.1
        ));
    }

    d
}

/// The same curve, evaluated rather than described, so it can be given width.
fn sample(points: &[(f32, f32)], per_segment: usize) -> Vec<(f32, f32)> {
    let mut out = Vec::new();

    for i in 0..points.len().saturating_sub(1) {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];

        for step in 0..per_segment {
            let t = step as f32 / per_segment as f32;
            let t2 = t * t;
            let t3 = t2 * t;

            out.push((
                0.5 * ((2.0 * p1.0)
                    + (-p0.0 + p2.0) * t
                    + (2.0 * p0.0 - 5.0 * p1.0 + 4.0 * p2.0 - p3.0) * t2
                    + (-p0.0 + 3.0 * p1.0 - 3.0 * p2.0 + p3.0) * t3),
                0.5 * ((2.0 * p1.1)
                    + (-p0.1 + p2.1) * t
                    + (2.0 * p0.1 - 5.0 * p1.1 + 4.0 * p2.1 - p3.1) * t2
                    + (-p0.1 + 3.0 * p1.1 - 3.0 * p2.1 + p3.1) * t3),
            ));
        }
    }

    if let Some(&last) = points.last() {
        out.push(last);
    }

    out
}

/// A closed shape that follows a curve and tapers along it. A stroke cannot
/// change width down its length; a limb that starts thick at the core and
/// thins to nothing at the category is what makes the map look grown instead
/// of wired, so the limbs are filled outlines rather than strokes.
fn taper(points: &[(f32, f32)], from: f32, to: f32) -> String {
    let curve = sample(points, 14);
    if curve.len() < 2 {
        return String::new();
    }

    let last = curve.len() - 1;
    let mut near = Vec::with_capacity(curve.len());
    let mut far = Vec::with_capacity(curve.len());

    for (i, &(x, y)) in curve.iter().enumerate() {
        let t = i as f32 / last as f32;
        let width = from + (to - from) * t;

        let before = curve[i.saturating_sub(1)];
        let after = curve[(i + 1).min(last)];
        let (dx, dy) = (after.0 - before.0, after.1 - before.1);
        let length = (dx * dx + dy * dy).sqrt().max(0.001);
        let (nx, ny) = (-dy / length, dx / length);

        near.push((x + nx * width * 0.5, y + ny * width * 0.5));
        far.push((x - nx * width * 0.5, y - ny * width * 0.5));
    }

    let mut d = format!("M{:.1},{:.1}", near[0].0, near[0].1);
    for point in near.iter().skip(1) {
        d.push_str(&format!("L{:.1},{:.1}", point.0, point.1));
    }
    for point in far.iter().rev() {
        d.push_str(&format!("L{:.1},{:.1}", point.0, point.1));
    }
    d.push('Z');

    d
}

/// Steers a limb from the title to a category without driving it through
/// anything already on the page.
///
/// Packing categories at whatever depth they fit means a limb often has to get
/// past two or three of them on the way. It is sampled along its length and, at
/// any sample that lands inside a category's box, pushed out through the nearer
/// side; the result is smoothed into a curve, so dodging reads as meandering
/// rather than as a detour.
fn route(
    from: (f32, f32),
    to: (f32, f32),
    obstacles: &[(f32, f32, f32, f32)],
    sway: f32,
    floor: f32,
    ceiling: f32,
) -> Vec<(f32, f32)> {
    let mut points = vec![from];
    let steps = 7;

    for step in 1..steps {
        let t = step as f32 / steps as f32;
        let x = from.0 + (to.0 - from.0) * t;

        // Close to the node the limb has to be allowed to arrive.
        if x > to.0 - 56.0 {
            break;
        }

        // Its own wave before it dodges anything, so two limbs escaping the
        // same obstacle the same way still travel as two strands.
        let mut y = from.1 + (to.1 - from.1) * t + drift(t * 2.6 + sway, 13.0);

        for _ in 0..4 {
            let mut clear = true;

            for &(bx, by, bw, bh) in obstacles {
                if x > bx - 10.0 && x < bx + bw + 10.0 && y > by - 8.0 && y < by + bh + 8.0 {
                    let up = y - (by - 12.0);
                    let down = (by + bh + 12.0) - y;
                    y = if up < down { by - 12.0 } else { by + bh + 12.0 };
                    clear = false;
                }
            }

            if clear {
                break;
            }
        }

        points.push((x, y.clamp(floor, ceiling)));
    }

    points.push(to);
    points
}

#[derive(Debug, Serialize)]
struct Leaf {
    /// CSS left. On the left-hand field the label grows away from the core, so
    /// this is already offset by the label's measured width.
    x: f32,
    y: f32,
    ico: String,
    lbl: String,
    url: String,
    vein: String,
}

/// The node standing for one separator-delimited group. It has no name in the
/// content, so it is drawn as a junction and nothing else.
#[derive(Debug, Serialize)]
struct Bud {
    x: f32,
    y: f32,
}

#[derive(Debug, Serialize)]
struct Branch {
    name: String,
    tone: usize,
    name_x: f32,
    name_y: f32,
    node_x: f32,
    node_y: f32,
    limb: String,
    stems: Vec<String>,
    buds: Vec<Bud>,
    leaves: Vec<Leaf>,
}

#[derive(Debug, Serialize)]
struct Other {
    name: String,
    path: String,
    x: f32,
    y: f32,
}

#[derive(Debug, Serialize)]
struct Map {
    width: f32,
    height: f32,
    core_x: f32,
    core_y: f32,
    branches: Vec<Branch>,
    others: Vec<Other>,
}

/// One separator-delimited run of links.
struct Group<'a> {
    leaves: Vec<(&'a str, &'a str, &'a str)>,
}

/// A category, measured but not yet placed.
struct Block<'a> {
    name: &'a str,
    tone: usize,
    fans: Vec<Vec<Group<'a>>>,
    fan_w: Vec<f32>,
    width: f32,
    height: f32,
}

/// Strips a leading "<category> |" from a label.
///
/// A category called FHP whose links all read "FHP | Bitbucket", "FHP |
/// Jenkins" says its own name nine times, and the category is already
/// labelled. Only stripped when every link carries it and it is the category's
/// own name, so nothing that distinguishes two links can be lost.
fn shorten<'a>(label: &'a str, category: &str) -> Option<&'a str> {
    let rest = label.strip_prefix(category)?.trim_start();
    let rest = rest.strip_prefix('|')?.trim_start();

    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// Splits every category into its groups and works out how much room it needs.
///
/// A category with more than a handful of links puts its groups into two fans
/// side by side rather than one long run. That is what keeps a map rooted in
/// the middle from growing taller than the screen.
fn measure(startpage: &Startpage) -> Vec<Block<'_>> {
    let mut blocks = Vec::new();

    for column in &startpage.columns {
        for category in &column.categories {
            let redundant = category
                .rows
                .iter()
                .filter_map(|row| match row {
                    Row::Link { lbl, .. } => Some(lbl),
                    Row::Separator => None,
                })
                .fold(None, |all: Option<bool>, lbl| {
                    Some(all.unwrap_or(true) && shorten(lbl, &category.name).is_some())
                })
                .unwrap_or(false);

            let mut groups: Vec<Group> = Vec::new();
            let mut current: Vec<(&str, &str, &str)> = Vec::new();

            for row in &category.rows {
                match row {
                    Row::Separator => {
                        if !current.is_empty() {
                            groups.push(Group {
                                leaves: std::mem::take(&mut current),
                            });
                        }
                    }
                    Row::Link { ico, lbl, url } => current.push((
                        ico,
                        if redundant {
                            shorten(lbl, &category.name).unwrap_or(lbl)
                        } else {
                            lbl
                        },
                        url,
                    )),
                }
            }
            if !current.is_empty() {
                groups.push(Group { leaves: current });
            }

            let total: usize = groups.iter().map(|group| group.leaves.len()).sum();
            // How many fans a category spreads over is decided by its own
            // shape rather than a fixed rule: add fans until the block stops
            // being taller than it is wide. Height is the scarce dimension, so
            // a tall narrow category is always worth trading for a squat one.
            let average = groups
                .iter()
                .flat_map(|group| group.leaves.iter())
                .map(|(_, lbl, _)| lbl.chars().count() as f32 * CH)
                .sum::<f32>()
                / total.max(1) as f32;

            let name_room =
                NAME_LEAD + category.name.chars().count() as f32 * NAME_CH + NAME_TRAIL;

            let mut wanted = 1;
            while wanted < 3 && wanted < groups.len() {
                let rows = (total as f32 / wanted as f32).ceil();
                let tall = rows * ROW + (groups.len() as f32 / wanted as f32) * GROUP_GAP;
                let wide = name_room + wanted as f32 * (BUD_DX + ICON + average + COL_PAD);

                if tall <= wide * 0.7 {
                    break;
                }
                wanted += 1;
            }

            let mut fans: Vec<Vec<Group>> = (0..wanted).map(|_| Vec::new()).collect();
            let target = total as f32 / wanted as f32;
            let mut at = 0;
            let mut carried = 0.0_f32;

            for group in groups {
                let size = group.leaves.len() as f32;
                if at + 1 < wanted && !fans[at].is_empty() && carried + size * 0.5 > target {
                    at += 1;
                    carried = 0.0;
                }
                carried += size;
                fans[at].push(group);
            }

            let fan_w: Vec<f32> = fans
                .iter()
                .map(|fan| {
                    let widest = fan
                        .iter()
                        .flat_map(|group| group.leaves.iter())
                        .map(|(_, lbl, _)| lbl.chars().count() as f32 * CH)
                        .fold(0.0_f32, f32::max);
                    BUD_DX + ICON + widest + COL_PAD
                })
                .collect();

            let height = fans
                .iter()
                .map(|fan| {
                    let rows: usize = fan.iter().map(|group| group.leaves.len()).sum();
                    rows as f32 * ROW + (fan.len().saturating_sub(1)) as f32 * GROUP_GAP
                })
                .fold(0.0_f32, f32::max);

            blocks.push(Block {
                name: &category.name,
                // A stride coprime with the palette, so consecutive categories
                // land on opposite sides of the wheel.
                tone: (blocks.len() * 4) % TONES,
                width: NAME_LEAD
                    + category.name.chars().count() as f32 * NAME_CH
                    + NAME_TRAIL
                    + fan_w.iter().sum::<f32>(),
                height,
                fans,
                fan_w,
            });
        }
    }

    blocks
}

/// Leftmost-fit packing against a skyline.
///
/// Categories are all at the same depth in the tree, but nothing says they have
/// to be drawn at the same x. Each one is dropped at the leftmost place it fits
/// beside what is already down, which staggers their depths and lets a short
/// category tuck into the space a tall neighbour leaves over. Returns the
/// top-left of every block plus the extent used.
fn pack(blocks: &[Block], budget: f32, hgap: f32, vgap: f32) -> (Vec<(f32, f32)>, f32, f32) {
    let slots = (budget / STEP).ceil() as usize + 2;
    let mut front = vec![0.0_f32; slots];
    let mut placed = Vec::with_capacity(blocks.len());
    let (mut right, mut bottom) = (0.0_f32, 0.0_f32);

    for block in blocks {
        let rows = ((block.height + vgap) / STEP).ceil() as usize;
        let reach = slots.saturating_sub(rows).max(1);

        let mut best = (f32::MAX, 0_usize);
        for start in 0..reach {
            let x = front[start..(start + rows).min(slots)]
                .iter()
                .cloned()
                .fold(0.0_f32, f32::max);
            if x < best.0 - 0.01 {
                best = (x, start);
            }
        }

        let (x, start) = best;
        let y = start as f32 * STEP;

        for slot in front
            .iter_mut()
            .take((start + rows).min(slots))
            .skip(start)
        {
            *slot = x + block.width + hgap;
        }

        placed.push((x, y));
        right = right.max(x + block.width);
        // The clearance separates a block from the next one down; counting it
        // here would pad the bottom of the field and steal the room the
        // categories want to spread into.
        bottom = bottom.max(y + block.height);
    }

    (placed, right, bottom)
}

/// Places the whole map: the title on the left, every category packed into the
/// space to the right of it, then every connector drawn.
fn plot(startpage: &Startpage, navigation: &Navigation) -> Map {
    let blocks = measure(startpage);

    let root_w = startpage.name.chars().count() as f32 * ROOT_CH;
    let field_x = MARGIN + root_w + LIMB_REACH;

    // Search over both the number of bands and how far apart the categories are
    // pushed, and keep the largest map that still fits the screen. Packing as
    // tightly as possible only leaves the map stranded in the corner of a
    // display that had room to spare - the slack belongs in the gaps.
    let mut best: Option<(Vec<(f32, f32)>, f32, f32, f32)> = None;
    let mut score = f32::MAX;

    // The two clearances are searched independently: a map with few, small
    // categories wants to breathe sideways and downwards by quite different
    // amounts, and tying them together leaves one axis short.
    for bands in 1..=5 {
        for across in 0..14 {
            for down in 0..28 {
                let hgap = HGAP * (1.0 + across as f32 * 0.32);
                let vgap = VGAP * (1.0 + down as f32 * 0.38);

                let tallest = blocks
                    .iter()
                    .map(|block| block.height + vgap)
                    .fold(0.0_f32, f32::max);
                let total: f32 = blocks
                    .iter()
                    .map(|block| block.height + vgap)
                    .sum::<f32>()
                    .max(1.0);

                let (places, right, bottom) =
                    pack(&blocks, (total / bands as f32).max(tallest), hgap, vgap);

                let w = field_x + right + MARGIN;
                let h = bottom + TOP_BAR + MARGIN * 2.0;

                // Overshooting is worse than undershooting, and overshooting
                // the height is worst of all: a map taller than the window has
                // to be scrolled, a wider one is only panned.
                let cost = (w - TARGET_W).max(0.0) * 3.0
                    + (h - TARGET_H).max(0.0) * 7.0
                    + (TARGET_W - w).max(0.0) * 2.0
                    + (TARGET_H - h).max(0.0) * 2.5;

                if cost < score {
                    score = cost;
                    best = Some((places, right, bottom, vgap));
                }
            }
        }
    }

    let (places, right, bottom, _vgap) = best.expect("at least one packing");

    // The field is a fixed height whatever this particular map needs, so the
    // title sits at the same point on every one of them and switching maps
    // never moves it. Content shorter than the field is centred in it.
    let field_h = bottom.max(TARGET_H - TOP_BAR - MARGIN * 2.0);
    let lift = TOP_BAR + MARGIN + (field_h - bottom) * 0.5;

    let width = field_x + right + MARGIN;
    let height = TOP_BAR + MARGIN + field_h + MARGIN;
    let core_x = MARGIN + root_w * 0.5;
    // Pinned to the target field rather than to this map's content, so the
    // title lands on the same line even on a map too big to fit it. Switching
    // maps then moves the tree and nothing else.
    let core_y = TOP_BAR + MARGIN + (TARGET_H - TOP_BAR - MARGIN * 2.0) * 0.5;

    // Every category's footprint, so the limbs can be steered around them.
    let rects: Vec<(f32, f32, f32, f32)> = blocks
        .iter()
        .zip(places.iter())
        .map(|(block, &(bx, by))| {
            (
                field_x + bx,
                lift + by,
                block.width,
                block.height,
            )
        })
        .collect();

    let mut branches = Vec::new();

    for (order, (block, &(bx, by))) in blocks.iter().zip(places.iter()).enumerate() {
        let node_x = field_x + bx;
        let node_y = lift + by + block.height * 0.5;
        let name_w = block.name.chars().count() as f32 * NAME_CH;

        // The limb leaves the title on the heading of its category, then finds
        // its way past anything packed in between.
        let origin = (core_x + root_w * 0.5 + 14.0, core_y);
        let barriers: Vec<(f32, f32, f32, f32)> = rects
            .iter()
            .enumerate()
            .filter(|(other, &(ox, _, ow, _))| *other != order && ox + ow < node_x - 12.0)
            .map(|(_, &rect)| rect)
            .collect();

        let path = route(
            origin,
            (node_x, node_y),
            &barriers,
            order as f32 * 1.7,
            MARGIN * 0.6,
            height - MARGIN * 0.6,
        );

        let mut stems = Vec::new();
        let mut buds = Vec::new();
        let mut leaves = Vec::new();

        let mut fan_x = node_x + NAME_LEAD + name_w + NAME_TRAIL;

        for (which, fan) in block.fans.iter().enumerate() {
            let fan_h: f32 = fan
                .iter()
                .map(|group| group.leaves.len() as f32 * ROW)
                .sum::<f32>()
                + (fan.len().saturating_sub(1)) as f32 * GROUP_GAP;

            let mut top = node_y - fan_h * 0.5;

            for group in fan {
                let run = group.leaves.len() as f32 * ROW;
                let bud = (fan_x, top + run * 0.5);

                stems.push(thread(&[
                    (node_x + 5.0, node_y),
                    (
                        node_x + (bud.0 - node_x) * 0.55,
                        node_y + (bud.1 - node_y) * 0.25,
                    ),
                    (bud.0 - 14.0, bud.1),
                    bud,
                ]));

                for (j, (ico, lbl, url)) in group.leaves.iter().enumerate() {
                    let leaf_y = top + j as f32 * ROW + ROW * 0.5;
                    let inner = bud.0 + BUD_DX;

                    leaves.push(Leaf {
                        x: px(inner),
                        y: px(leaf_y),
                        ico: (*ico).to_owned(),
                        lbl: (*lbl).to_owned(),
                        url: (*url).to_owned(),
                        vein: thread(&[
                            (bud.0 + 4.0, bud.1),
                            (bud.0 + BUD_DX * 0.5, bud.1 + (leaf_y - bud.1) * 0.55),
                            (inner - 6.0, leaf_y),
                        ]),
                    });
                }

                buds.push(Bud {
                    x: px(bud.0),
                    y: px(bud.1),
                });

                top += run + GROUP_GAP;
            }

            fan_x += block.fan_w[which];
        }

        branches.push(Branch {
            name: block.name.to_owned(),
            tone: block.tone,
            name_x: px(node_x + NAME_LEAD),
            name_y: px(node_y),
            node_x: px(node_x),
            node_y: px(node_y),
            limb: taper(&path, 5.5, 1.4),
            stems,
            buds,
            leaves,
        });
    }

    // The other maps are navigation, not part of this tree, so they sit in a
    // strip above it rather than hanging off the title.
    let elsewhere: Vec<&NavItem> = navigation
        .iter()
        .filter(|nav| nav.name != startpage.name)
        .collect();

    let mut others = Vec::new();
    let mut at = MARGIN;
    for nav in elsewhere {
        others.push(Other {
            name: nav.name.to_owned(),
            path: nav.path.to_owned(),
            x: px(at),
            y: 32.0,
        });
        at += nav.name.chars().count() as f32 * OTHER_CH + 34.0;
    }

    Map {
        width: px(width),
        height: px(height),
        core_x: px(core_x),
        core_y: px(core_y),
        branches,
        others,
    }
}

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("templates/**/*.html") {
            Ok(t) => t,
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![".html"]);
        tera
    };
}

/// It parses YAML configuration into internal Rust structure.
fn get_startpages() -> Result<Vec<Startpage>, Box<dyn std::error::Error>> {
    let mut file = File::open(CONFIGURATION)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let startpages: Vec<Startpage> = serde_yaml::from_str(&contents)?;

    Ok(startpages)
}

/// It collects names of startpages and their file paths, keeping the order in
/// which the startpages are defined in the configuration.
fn get_navigation(startpages: &Vec<Startpage>) -> Navigation {
    startpages
        .iter()
        .map(|startpage| {
            let startpage_safe_name = startpage.name.to_lowercase().replace(" ", "_");

            NavItem {
                name: startpage.name.to_owned(),
                path: format!("{}.html", startpage_safe_name),
            }
        })
        .collect()
}

/// Recreates output directory.
fn prepare_out_dir() -> Result<(), Box<dyn std::error::Error>> {
    if fs::metadata(OUT_DIR).is_ok() {
        fs::remove_dir_all(OUT_DIR)?;
    }
    fs::create_dir(OUT_DIR)?;

    Ok(())
}

/// For each startpage it lays out the map and renders it through the template.
fn generate_startpages(
    startpages: &Vec<Startpage>,
    navigation: &Navigation,
) -> Result<(), Box<dyn std::error::Error>> {
    for startpage in startpages {
        let mut context = tera::Context::new();
        context.insert("startpage", &startpage);
        context.insert("map", &plot(startpage, navigation));

        let html_code = TEMPLATES.render("startpage.html", &context)?;

        let startpage_file_name = &navigation
            .iter()
            .find(|nav| nav.name == startpage.name)
            .ok_or("Navigation doesn't contain startpage name")?
            .path;

        let startpage_file_path = format!("{}/{}", OUT_DIR, startpage_file_name);
        let mut file = File::create(startpage_file_path)?;

        file.write_all(html_code.as_bytes())?;
    }

    Ok(())
}

/// Compiles sass/styles.scss to _site/css/styles.css
fn compile_sass() -> Result<(), Box<dyn std::error::Error>> {
    let css_dir = format!("{}/css", OUT_DIR);
    fs::create_dir(&css_dir)?;

    let path = "sass/styles.scss".as_ref();
    let format = output::Format {
        ..Default::default()
    };
    let css = compile_scss_path(path, format)?;

    let css_file_path = format!("{}/styles.css", css_dir);
    let mut file = File::create(css_file_path)?;

    file.write_all(&css.as_slice())?;

    Ok(())
}

/// Copies public dir to _site/public
fn copy_public_dir() -> Result<(), Box<dyn std::error::Error>> {
    let options = CopyOptions::new();
    copy("public", OUT_DIR, &options)?;

    Ok(())
}

fn main() {
    let startpages = get_startpages().expect("Failed to parse startpages content");
    println!("Startpages content parsed");

    let navigation = get_navigation(&startpages);
    println!("Navigation extracted from the content");

    prepare_out_dir().expect("Failed to prepare output directory");
    println!("Output directory prepared");

    generate_startpages(&startpages, &navigation).expect("Failed to generate startpages");
    println!("Maps plotted and startpages generated");

    compile_sass().expect("Failed to compile Sass");
    println!("Sass styles compiled");

    copy_public_dir().expect("Failed to copy public directory");
    println!("Public directory copied");
}
