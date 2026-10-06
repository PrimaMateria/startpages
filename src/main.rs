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
static COLORSCHEMES: &str = "sass/colorschemes";
/// The classic layout's theme until one is picked in the settings.
static DEFAULT_THEME: &str = "gruvbox-dark-hard";

/// A base16 colour scheme, offered as a theme for the classic layout.
#[derive(Debug, Serialize)]
struct Theme {
    id: String,
    name: String,
    #[serde(skip)]
    colours: Vec<(String, String)>,
}

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
const CH: f32 = 9.0;
/// Advance width of one character in a category name, tracking included.
const NAME_CH: f32 = 13.2;
/// Advance width of one character in the title.
const ROOT_CH: f32 = 16.7;
/// Advance width of one character in a link to another map.
const OTHER_CH: f32 = 8.6;

/// Vertical pitch between leaves.
const ROW: f32 = 26.0;
/// Space between one bud's leaves and the next bud's.
const GROUP_GAP: f32 = 25.0;
/// Room the icon and its gap occupy beside a label.
const ICON: f32 = 26.0;
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
const TARGET_W: f32 = 1950.0;
const TARGET_H: f32 = 1000.0;
/// Drop from a row's baseline to the category node sitting on it.
const NODE_DROP: f32 = 16.0;
/// How far a row bows across its length.
const ARC: f32 = 9.0;
/// How far every other category on a row is dropped below its neighbours.
///
/// A row of categories all at one height gives the branch above it nothing to
/// fork into: whatever the skeleton does up there collapses into a flat bus
/// with ticks hanging off it. Staggering them is what lets a fork be a Y.
const STAGGER: f32 = 26.0;
/// Drop from a branch to the top of the cluster under it.
const HANG: f32 = 46.0;
/// Category node to the first fan hanging off it.
const FAN_LEAD: f32 = 32.0;
/// How far a limb reaches before it arrives at the first category.
const LIMB_REACH: f32 = 214.0;
/// Width of the trunk where it leaves the title.
const TRUNK_W: f32 = 10.0;
/// Width where a branch finally meets a category.
const TIP_W: f32 = 1.5;
/// How much of its width a segment keeps when it forks.
const TAPER: f32 = 0.66;

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
    // Enough samples to read as a curve, without re-sampling an already-dense
    // routed path into several kilobytes of path data.
    let per_segment = (56 / points.len().max(1)).clamp(2, 14);
    let curve = sample(points, per_segment);
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
    stems: Vec<String>,
    buds: Vec<Bud>,
    leaves: Vec<Leaf>,
}

/// One segment of the skeleton: trunk, branch, or the twig that finally reaches
/// a category. Its width says how far from the root it is.
#[derive(Debug, Serialize)]
struct Bough {
    d: String,
    /// -1 while the segment is still carrying more than one category; the
    /// category's own tone once it carries only that one.
    tone: i32,
}

/// The same nine tones as `:root` in sass/styles.scss, so the field can carry a
/// trace of the palette. KEEP IN STEP WITH THE STYLESHEET.
/// How far past the map the background field is traced, so that it still
/// covers a screen wider and taller than the map itself.
const BLEED_W: f32 = 2800.0;
const BLEED_H: f32 = 1600.0;

const TONE_RGB: [[f32; 3]; TONES] = [
    [106.0, 236.0, 150.0],
    [255.0, 159.0, 190.0],
    [203.0, 173.0, 255.0],
    [122.0, 194.0, 255.0],
    [255.0, 153.0, 233.0],
    [178.0, 255.0, 96.0],
    [78.0, 230.0, 230.0],
    [255.0, 166.0, 74.0],
    [255.0, 212.0, 92.0],
];

/// One line of the field the tree stands in.
#[derive(Debug, Serialize)]
struct Strand {
    d: String,
    ink: f32,
    tint: String,
}

/// A set of strings that breathe together.
///
/// The breathing used to be declared on every string, each with its own delay.
/// That is one animating element per string - up to a hundred of them, each
/// with a bounding box the size of the field - and the browser answers an
/// animation it cannot composite by repainting that area every frame. Sharing
/// one animation between a handful of groups buys back nearly all of it and
/// still keeps the drift, since the groups are out of phase with each other.
#[derive(Debug, Serialize)]
struct Breath {
    beat: f32,
    strands: Vec<Strand>,
}

/// A brightening that travels the length of one string.
#[derive(Debug, Serialize)]
struct Ripple {
    d: String,
    tint: String,
    wake: f32,
}

/// Sort the field into groups that breathe together, and choose the few strings
/// that carry a ripple.
fn weave(strands: Vec<Strand>) -> (Vec<Breath>, Vec<Ripple>) {
    /// Enough that the field drifts rather than pulsing in one body, few enough
    /// that the page is not repainting itself for each one.
    const BREATHS: usize = 4;
    /// `Occasional` was the brief. One ripple per four strings is a rhythm, and
    /// on a wide page it was twenty of them at once.
    const CARRIED: usize = 3;

    let mut ripples = Vec::new();
    if !strands.is_empty() {
        for nth in 0..CARRIED.min(strands.len()) {
            // Spread along the field, and spread around the cycle, so they
            // neither cluster in one corner nor arrive together.
            let pick = (nth * 2 + 1) * strands.len() / (CARRIED * 2);
            ripples.push(Ripple {
                d: strands[pick].d.clone(),
                tint: strands[pick].tint.clone(),
                wake: nth as f32 * 54.0 / CARRIED as f32,
            });
        }
    }

    let mut field: Vec<Breath> = (0..BREATHS)
        .map(|nth| Breath {
            beat: nth as f32 * 19.0 / BREATHS as f32,
            strands: Vec::new(),
        })
        .collect();
    for (nth, strand) in strands.into_iter().enumerate() {
        field[nth % BREATHS].strands.push(strand);
    }

    (field, ripples)
}

/// How far a point is from the nearest thing already on the canvas.
fn clearance(point: (f32, f32), taken: &[(f32, f32, f32, f32)]) -> f32 {
    // Blended rather than a plain `min`. Where two clusters are equally close
    // the minimum of their distances has a crease running between them, and a
    // contour crossing that crease comes out with a sharp corner - or, if the
    // corrector oscillates across it, a little tangle of spikes. Rounding the
    // join by a blend radius removes the crease, so every contour is smooth
    // wherever it runs.
    const BLEND: f32 = 26.0;
    let mut room = f32::MAX;

    for &(bx, by, bw, bh) in taken {
        let dx = (bx - point.0).max(point.0 - (bx + bw)).max(0.0);
        let dy = (by - point.1).max(point.1 - (by + bh)).max(0.0);
        let reach = (dx * dx + dy * dy).sqrt();

        let blend = ((BLEND - (room - reach).abs()) / BLEND).max(0.0);
        room = room.min(reach) - blend * blend * BLEND * 0.25;
    }

    // Far from the tree a plain distance field has nothing left to describe,
    // and its contours straighten into a rounded rectangle around the content
    // - long vertical runs down the side of the page that read as a wall
    // rather than as strings. A slow warp keeps them moving.
    //
    // Its amplitude grows with distance, so the close-in family that traces
    // the crown is left almost untouched while the far field, where the levels
    // are furthest apart and there is room to wander, gets all of it. Because
    // the warp is part of the field rather than an offset applied afterwards,
    // the strings are still contours of one scalar field, and contours of one
    // field cannot cross.
    let sway = (room * 0.10).min(55.0);

    room + sway * (point.0 / 263.0).sin() * (point.1 / 197.0).sin()
}

/// Which way clearance increases, by central difference.
fn uphill(point: (f32, f32), taken: &[(f32, f32, f32, f32)]) -> (f32, f32) {
    const H: f32 = 7.0;
    let dx = clearance((point.0 + H, point.1), taken) - clearance((point.0 - H, point.1), taken);
    let dy = clearance((point.0, point.1 + H), taken) - clearance((point.0, point.1 - H), taken);
    (dx, dy)
}

/// Traces the strings of the field the tree stands in.
///
/// Not shapes dropped into gaps: each string is a contour of the distance to
/// everything already drawn, so it wraps the tree's actual silhouette at a
/// fixed remove and fills whatever outer space that leaves. Stepping along the
/// contour drifts off it, so every step is followed by a correction back onto
/// the level.
fn field_lines(
    width: f32,
    height: f32,
    taken: &[(f32, f32, f32, f32)],
    crown: &[(f32, f32, f32, f32)],
    nodes: &[(f32, f32, usize)],
) -> Vec<Strand> {
    // Shorter strides and a firmer correction: a long stride across a corner of
    // the field lands on a different level, and strings that have swapped
    // levels cross each other. Contours of one field never do.
    const STEP: f32 = 32.0;
    // A family at close intervals, not a handful of marks: the eye reads a run
    // of near-parallel lines as texture, and three lines as three lines.
    const LEVELS: usize = 22;
    // The first level sets how close the field comes to the tree. It should
    // give the tree more room than it gives the frame.
    const FIRST: f32 = 98.0;
    const APART: f32 = 25.0;

    // Keep the field to the space outside the tree. A contour started in among
    // the clusters would draw an outline around the content instead.
    //
    // Measured from the clusters alone: `taken` also holds the full-width strip
    // at the top of the page, which would make the hull the whole canvas and
    // leave nowhere counted as outside at all.
    let (mut left, mut top, mut right, mut foot) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(bx, by, bw, bh) in crown {
        left = left.min(bx);
        top = top.min(by);
        right = right.max(bx + bw);
        foot = foot.max(by + bh);
    }

    let mut out = Vec::new();

    for order in 0..LEVELS {
        // Spacing widens with distance. Even spacing over the smooth,
        // monotonic field out past the crown lays down a comb of parallel
        // lines at a constant gap, which reads as hatching; letting the gap
        // grow keeps the strings tight where they trace the tree and lets
        // them thin out into the corners.
        // Spacing widens with distance, and the cubic is what does the work
        // out past the crown. Even spacing over the smooth, monotonic far
        // field lays down a comb of parallel lines at a constant gap, which
        // reads as hatching; worse, the far field is the largest area on the
        // page, so a constant gap puts the most ink where there is least to
        // say. The square term alone still left twenty lines down the right
        // margin. The cubic is negligible for the first few levels, which are
        // the ones that trace the crown, and dominant by the last.
        let step = order as f32;
        let level = FIRST + step * APART + step * step * 1.6 + step * step * step * 0.22;
        let mut started: Vec<(f32, f32)> = Vec::new();

        let mut y = 70.0;
        while y < height - 70.0 {
            let mut x = 70.0;
            while x < width - 70.0 {
                let here = clearance((x, y), taken);
                let near = started
                    .iter()
                    .any(|&(sx, sy)| ((x - sx).powi(2) + (y - sy).powi(2)).sqrt() < 330.0);

                // A level's band is only as wide as the tolerance, so a coarse
                // scan walks straight over it and the family comes out with
                // two members instead of twelve.
                // A contour that closes on itself reads as a box outline
                // rather than a string, so each is traced outward from its seed
                // in both directions and capped - an open arc, not a loop.
                if (here - level).abs() < 13.0 // Capped per level, but high enough that the scan does not spend
                // its whole budget on the first side it reaches.
                && !near
                    && started.len() < 6
                {
                    started.push((x, y));

                    let seed = (x, y);
                    let mut line: Vec<(f32, f32)> = Vec::new();

                    for way in [1.0_f32, -1.0] {
                        let mut at = seed;
                        let mut leg = vec![at];

                        for _ in 0..84 {
                            let slope = uphill(at, taken);
                            let steep = (slope.0 * slope.0 + slope.1 * slope.1).sqrt();
                            if steep < 0.001 {
                                break;
                            }

                            // Along the contour is across the slope.
                            at = (
                                at.0 - slope.1 / steep * STEP * way,
                                at.1 + slope.0 / steep * STEP * way,
                            );

                            // Then pull back onto the level it wandered off.
                            let drift = level - clearance(at, taken);
                            let slope = uphill(at, taken);
                            let steep =
                                (slope.0 * slope.0 + slope.1 * slope.1).sqrt().max(0.001);
                            at = (
                                at.0 + slope.0 / steep * drift,
                                at.1 + slope.1 / steep * drift,
                            );

                            if at.0 < 30.0
                                || at.0 > width - 30.0
                                || at.1 < 30.0
                                || at.1 > height - 30.0
                            {
                                break;
                            }

                            leg.push(at);
                        }

                        if way > 0.0 {
                            leg.reverse();
                            line = leg;
                        } else {
                            line.extend(leg.into_iter().skip(1));
                        }
                    }

                    // Anything this short is not a string, it is debris: a
                    // trace that hit a wall a few steps after it started.
                    // Drop only the near-closed traces, which read as loops
                    // rather than strings. A contour that legitimately wraps
                    // half the crown also has its ends close together, so the
                    // bar has to sit low or the whole family goes with it.
                    let span = {
                        let a = line[0];
                        let b = line[line.len() - 1];
                        ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
                    };
                    let travelled = line.len() as f32 * STEP;

                    // A trace can also curl up inside a pocket between the
                    // title and the trunk and come out long but tiny. Reach
                    // catches those where span cannot, since a curl has its
                    // ends far apart relative to how little ground it covers.
                    let reach = {
                        let left = line.iter().fold(f32::MAX, |m, p| m.min(p.0));
                        let right = line.iter().fold(f32::MIN, |m, p| m.max(p.0));
                        let top = line.iter().fold(f32::MAX, |m, p| m.min(p.1));
                        let foot = line.iter().fold(f32::MIN, |m, p| m.max(p.1));
                        ((right - left).powi(2) + (foot - top).powi(2)).sqrt()
                    };

                    if line.len() > 18 && span > travelled * 0.12 && reach > 210.0 {
                        let thinned: Vec<(f32, f32)> =
                            line.iter().step_by(3).cloned().collect();
                        // A trace of whatever tone the string runs nearest,
                        // folded into a neutral. Far too little to name the
                        // colour, enough that the field belongs to the tree.
                        const NEUTRAL: [f32; 3] = [150.0, 166.0, 186.0];
                        let middle = line[line.len() / 2];
                        let tint = nodes
                            .iter()
                            .min_by(|a, b| {
                                let reach = |n: &(f32, f32, usize)| {
                                    (n.0 - middle.0).powi(2) + (n.1 - middle.1).powi(2)
                                };
                                reach(a).partial_cmp(&reach(b)).unwrap()
                            })
                            .map(|&(_, _, tone)| {
                                let hue = TONE_RGB[tone % TONES];
                                format!(
                                    "{:.0} {:.0} {:.0}",
                                    NEUTRAL[0] * 0.72 + hue[0] * 0.28,
                                    NEUTRAL[1] * 0.72 + hue[1] * 0.28,
                                    NEUTRAL[2] * 0.72 + hue[2] * 0.28
                                )
                            })
                            .unwrap_or_else(|| "150 166 186".to_owned());

                        out.push(Strand {
                            d: thread(&thinned),
                            // Nearer the tree reads a touch stronger, so the
                            // field falls away rather than stopping flat.
                            ink: (0.15 - order as f32 * 0.008).max(0.05),
                            tint,
                        });
                    }
                }

                x += 13.0;
            }
            y += 13.0;
        }
    }

    out
}

#[derive(Debug, Serialize)]
struct Other {
    name: String,
    path: String,
    x: f32,
    y: f32,
    /// Whether this is the map being looked at. Marked, but never left out -
    /// dropping it would shift every other name along.
    here: bool,
}

#[derive(Debug, Serialize)]
struct Map {
    width: f32,
    height: f32,
    field: Vec<Breath>,
    ripples: Vec<Ripple>,
    core_x: f32,
    core_y: f32,
    boughs: Vec<Bough>,
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

            // A run of a dozen links with no separator in it makes a fan tall
            // enough to set the height of its whole row, which forces the crown
            // wider or taller than it has any need to be. Long runs are halved
            // so the packer can put them side by side instead of end to end.
            let mut split: Vec<Group> = Vec::new();
            for group in groups {
                if group.leaves.len() > 8 {
                    let half = group.leaves.len().div_ceil(2);
                    let mut leaves = group.leaves;
                    let tail = leaves.split_off(half);
                    split.push(Group { leaves });
                    split.push(Group { leaves: tail });
                } else {
                    split.push(group);
                }
            }
            let groups = split;

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
                // The name hangs over the fans rather than beside them, so a
                // category is only as wide as its fans unless the name itself
                // is longer.
                width: (FAN_LEAD + fan_w.iter().sum::<f32>())
                    .max(NAME_LEAD + category.name.chars().count() as f32 * NAME_CH + 26.0),
                height,
                fans,
                fan_w,
            });
        }
    }

    blocks
}

/// Lays the categories out as a crown.
///
/// The rows are not all the same length: each aims at a share of the content
/// proportional to how wide an ellipse is at that height, so the middle rows
/// are long and the top and bottom ones short. Centring them then gives the
/// whole thing a rounded silhouette - and, just as usefully, leaves the room on
/// the left that the branches need to fan out through.
///
/// Returns each category's row and offset along it, and each row's height and
/// length.
fn crown(blocks: &[Block], rows: usize, hgap: f32) -> (Vec<(usize, f32)>, Vec<f32>, Vec<f32>) {
    let count = rows.max(1);

    let shape: Vec<f32> = (0..count)
        .map(|r| {
            let t = if count == 1 {
                0.0
            } else {
                2.0 * r as f32 / (count - 1) as f32 - 1.0
            };
            (1.0 - (t * 0.88).powi(2)).max(0.16).sqrt()
        })
        .collect();

    let mut left: f32 = blocks.iter().map(|block| block.width + hgap).sum::<f32>() - hgap;
    let mut share: f32 = shape.iter().sum();

    let mut placed = Vec::with_capacity(blocks.len());
    let mut heights = vec![0.0_f32; count];
    let mut widths = vec![0.0_f32; count];
    let mut row = 0usize;
    let mut along = 0.0_f32;

    for (index, block) in blocks.iter().enumerate() {
        let target = left * shape[row] / share.max(0.001);
        let rows_after = count - row - 1;

        // Move on either because this row has had its share, or because there
        // are only just enough categories left to give every remaining row one.
        // Without the second test a row can end up empty and still cost the
        // crown its full height.
        let had_enough = along + block.width * 0.5 > target;
        let must_move = blocks.len() - index <= rows_after;

        if along > 0.0 && row + 1 < count && (must_move || had_enough) {
            left -= along;
            share -= shape[row];
            row += 1;
            along = 0.0;
        }

        heights[row] = heights[row].max(block.height);
        placed.push((row, along));
        along += block.width + hgap;
        widths[row] = along - hgap;
    }

    (placed, heights, widths)
}

/// Nudges a point clear of anything it has landed inside.
///
/// `bias` is the direction the segment is already travelling. Escaping to
/// whichever side happens to be nearer makes a branch zigzag around one cluster
/// and back around the next; committing to the direction it was going anyway
/// turns the same dodge into a single arc over or under.
fn clear_of(point: (f32, f32), obstacles: &[(f32, f32, f32, f32)], bias: f32) -> (f32, f32) {
    let (x, mut y) = point;

    for _ in 0..5 {
        let mut clear = true;
        for &(bx, by, bw, bh) in obstacles {
            if x > bx - 10.0 && x < bx + bw + 10.0 && y > by - 8.0 && y < by + bh + 8.0 {
                y = if bias < 0.0 { by - 16.0 } else { by + bh + 16.0 };
                clear = false;
            }
        }
        if clear {
            break;
        }
    }

    (x, y)
}

/// Steers a segment from one point to another without driving it through a
/// cluster.
///
/// Displacing every sample that lands inside something is the wrong model: it
/// builds a tent around each one, and tents from neighbouring clusters pile up
/// into spikes. What the segment actually needs is a handful of deliberate
/// waypoints - so the clusters the straight line crosses are found, merged into
/// runs, and each run gets one apex to arc over. Few points in, smooth curve
/// out.
fn route(
    from: (f32, f32),
    to: (f32, f32),
    obstacles: &[(f32, f32, f32, f32)],
    sway: f32,
) -> Vec<(f32, f32)> {
    const PROBES: usize = 28;

    // Committing to the direction the segment is already travelling turns a
    // dodge into a single arc rather than a zigzag.
    let bias = if to.1 >= from.1 { 1.0 } else { -1.0 };

    // Where the straight line is blocked, and how far clear it would have to be.
    let mut blocked: Vec<Option<f32>> = Vec::with_capacity(PROBES + 1);

    for i in 0..=PROBES {
        let t = i as f32 / PROBES as f32;
        let x = from.0 + (to.0 - from.0) * t;
        let y = from.1 + (to.1 - from.1) * t;
        let mut clear: Option<f32> = None;

        for &(bx, by, bw, bh) in obstacles {
            let hit = x > bx - 14.0
                && x < bx + bw + 14.0
                && y > by - 12.0
                && y < by + bh + 12.0;

            if hit {
                let edge = if bias < 0.0 { by - 20.0 } else { by + bh + 20.0 };
                clear = Some(match clear {
                    Some(had) if (had - y).abs() > (edge - y).abs() => had,
                    _ => edge,
                });
            }
        }

        blocked.push(clear);
    }

    // One apex per run of blocked probes.
    let mut points = vec![from];
    let mut run: Option<(usize, f32)> = None;

    for i in 0..=PROBES {
        match (blocked[i], run) {
            (Some(edge), None) => run = Some((i, edge)),
            (Some(edge), Some((start, worst))) => {
                let keep = if (edge - from.1).abs() > (worst - from.1).abs() {
                    edge
                } else {
                    worst
                };
                run = Some((start, keep));
            }
            (None, Some((start, worst))) => {
                let mid = (start + i - 1) as f32 * 0.5 / PROBES as f32;
                points.push((
                    from.0 + (to.0 - from.0) * mid,
                    worst + drift(mid * 3.0 + sway, 6.0),
                ));
                run = None;
            }
            (None, None) => {}
        }
    }

    if let Some((start, worst)) = run {
        let mid = (start + PROBES) as f32 * 0.5 / PROBES as f32;
        points.push((
            from.0 + (to.0 - from.0) * mid,
            worst + drift(mid * 3.0 + sway, 6.0),
        ));
    }

    // Nothing in the way: still give the segment a little belly so it reads as
    // grown rather than ruled.
    if points.len() == 1 {
        points.push((
            from.0 + (to.0 - from.0) * 0.5,
            from.1 + (to.1 - from.1) * 0.5 + drift(sway, 13.0),
        ));
    }

    points.push(to);

    // A long clear stretch between two waypoints comes out ruler-straight;
    // give it a little belly so the whole skeleton reads as grown.
    let mut bellied = vec![points[0]];
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let reach = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        if reach > 240.0 {
            bellied.push((
                (a.0 + b.0) * 0.5,
                (a.1 + b.1) * 0.5 + drift(a.0 * 0.004 + sway, 14.0),
            ));
        }
        bellied.push(b);
    }

    bellied
}

/// Eases a segment out of the joint it starts from.
///
/// Each segment is its own filled outline, so where a parent ends and two
/// children begin the three square ends meet at an angle and leave a notch.
/// Backing each child up a little into its parent fills that in, and giving it
/// a first waypoint along the direction the parent arrived on means it leaves
/// the joint on the same tangent - which is what turns a kink into a Y.
fn ease_out(path: &mut Vec<(f32, f32)>, heading: (f32, f32)) {
    if path.len() < 2 {
        return;
    }

    let root = path[0];
    let next = path[1];
    let (dx, dy) = (next.0 - root.0, next.1 - root.1);
    let gap = (dx * dx + dy * dy).sqrt();
    if gap < 1.0 {
        return;
    }

    // Only ease a child that is carrying on roughly the way its parent
    // arrived. Backing a sharply turning one into its parent makes the path
    // double back on itself, and a curve through a reversal loops.
    // 0.3 still lets a near-reversal through, and the back-step then ties a
    // small knot in the path. Half is the point where the child is genuinely
    // carrying on the parent's way.
    let along = (dx / gap) * heading.0 + (dy / gap) * heading.1;
    if along < 0.5 {
        return;
    }

    if gap > 46.0 {
        path.insert(1, (root.0 + heading.0 * 17.0, root.1 + heading.1 * 17.0));
    }

    path.insert(0, (root.0 - heading.0 * 5.0, root.1 - heading.1 * 5.0));
}

/// The direction a path is travelling as it arrives.
fn heading_of(path: &[(f32, f32)]) -> (f32, f32) {
    if path.len() < 2 {
        return (1.0, 0.0);
    }
    let a = path[path.len() - 2];
    let b = path[path.len() - 1];
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    (dx / len, dy / len)
}

/// Grows the skeleton to every category by halving, so every joint is a fork
/// of two and never a run of one.
///
/// The set still to be reached is split across whichever axis it is more spread
/// over - up and down while the branch is still serving several rows, left and
/// right once it is down to one - and the fork is placed behind all of them.
#[allow(clippy::too_many_arguments)]
fn branch_out(
    members: &[usize],
    nodes: &[(f32, f32, usize)],
    rows: &[usize],
    lanes: &[f32],
    corridor: f32,
    rects: &[(f32, f32, f32, f32)],
    from: (f32, f32),
    heading: (f32, f32),
    width: f32,
    depth: usize,
    boughs: &mut Vec<Bough>,
) {
    if members.is_empty() {
        return;
    }

    if members.len() == 1 {
        let (nx, ny, tone) = nodes[members[0]];

        // Everything except the cluster it is arriving at.
        let others: Vec<(f32, f32, f32, f32)> = rects
            .iter()
            .enumerate()
            .filter(|(which, _)| *which != members[0])
            .map(|(_, &rect)| rect)
            .collect();

        let mut path: Vec<(f32, f32)> = route((nx, ny), from, &others, depth as f32 * 1.7)
            .into_iter()
            .rev()
            .collect();
        ease_out(&mut path, heading);

        boughs.push(Bough {
            d: taper(&path, width, TIP_W),
            tone: tone as i32,
        });
        return;
    }

    // Split whole rows off while the branch still serves more than one, and
    // only split left from right once it is down to a single row.
    //
    // Splitting on raw spread lets a subtree hold categories from opposite ends
    // of the crown, and a fork has to sit behind all of its members - so the
    // branch is dragged back across everything in between. Keeping a subtree to
    // whole rows, or to one run within a row, keeps every fork local to what it
    // feeds.
    let mut sorted = members.to_vec();
    sorted.sort_by(|&a, &b| {
        rows[a]
            .cmp(&rows[b])
            .then(nodes[a].0.partial_cmp(&nodes[b].0).unwrap())
    });

    let spans = {
        let mut seen: Vec<usize> = sorted.iter().map(|&m| rows[m]).collect();
        seen.dedup();
        seen.len()
    };

    let half = if spans > 1 {
        // Cut on a row boundary, whichever is nearest the middle.
        let mut best = sorted.len() / 2;
        let mut closest = usize::MAX;
        for cut in 1..sorted.len() {
            if rows[sorted[cut]] != rows[sorted[cut - 1]] {
                let off = cut.abs_diff(sorted.len() / 2);
                if off < closest {
                    closest = off;
                    best = cut;
                }
            }
        }
        best
    } else {
        sorted.len() / 2
    };

    let nearest = members.iter().map(|&m| nodes[m].0).fold(f32::MAX, f32::min);
    let span = (nearest - from.0).max(52.0);
    let point = (
        (from.0 + span * 0.46).min(nearest - 20.0).max(from.0 + 14.0),
        members.iter().map(|&m| nodes[m].1).sum::<f32>() / members.len() as f32,
    );

    // Once a branch is down to categories on one row, its forks belong in the
    // clear lane just above that row rather than out to the left of it. A fork
    // has to sit behind everything it feeds, so left of the row means the twig
    // to the far end has to cross every cluster in between; up in the lane it
    // travels over open ground and drops straight down onto each node.
    // The higher in the lane a fork sits, the more it still carries - so the
    // little tree above a row has visible depth instead of collapsing into one
    // flat bus with ticks hanging off it.
    let single = sorted.windows(2).all(|pair| rows[pair[0]] == rows[pair[1]]);
    let point = if single {
        let load = (sorted.len() as f32).min(6.0);
        let rise = (14.0 + load * 6.0).min(corridor - 10.0).max(14.0);
        (point.0, lanes[rows[sorted[0]]] - rise)
    } else {
        point
    };

    let point = clear_of(point, rects, if point.1 >= from.1 { 1.0 } else { -1.0 });

    let next = (width * TAPER).max(TIP_W + 0.3);
    let mut path = route(from, point, rects, depth as f32 * 1.3);
    ease_out(&mut path, heading);
    let onward = heading_of(&path);

    boughs.push(Bough {
        d: taper(&path, width, next),
        tone: -1,
    });

    for side in [&sorted[..half], &sorted[half..]] {
        branch_out(
            side, nodes, rows, lanes, corridor, rects, point, onward, next, depth + 1,
            boughs,
        );
    }
}

/// Places the whole map: a crown of categories, and one skeleton grown to
/// reach them all.
fn plot(startpage: &Startpage, navigation: &Navigation) -> Map {
    let blocks = measure(startpage);

    let root_w = startpage.name.chars().count() as f32 * ROOT_CH;
    let field_x = MARGIN + root_w + LIMB_REACH;

    let mut best: Option<(Vec<(usize, f32)>, Vec<f32>, Vec<f32>, f32)> = None;
    let mut score = f32::MAX;

    // Four rows at most. Beyond that the crown stops being a crown: it grows
    // taller than the title it hangs off, and since the title is pinned the
    // trunk turns into a long vertical spine up the left-hand side.
    for rows in 1..=4.min(blocks.len().max(1)) {
        for across in 0..13 {
            for down in 0..16 {
                let hgap = HGAP * (1.0 + across as f32 * 0.3);
                let vgap = VGAP * (1.0 + down as f32 * 0.42);

                let (placed, heights, widths) = crown(&blocks, rows, hgap);

                let tall: f32 = heights.iter().map(|h| h + HANG + STAGGER).sum::<f32>()
                    + vgap * (heights.len().saturating_sub(1)) as f32;
                let wide = widths.iter().cloned().fold(0.0_f32, f32::max);

                let w = field_x + wide + MARGIN;
                let h = tall + TOP_BAR + MARGIN * 2.0;

                // The densest map needs more area than a screen has, so the
                // question is only where the overflow goes. Width costs more
                // than height: too tall is a mouse wheel, too wide is a
                // horizontal pan, and panning is the worse of the two.
                let cost = (w - TARGET_W).max(0.0) * 5.0
                    + (h - TARGET_H).max(0.0) * 5.0
                    + (TARGET_W - w).max(0.0) * 2.0
                    + (TARGET_H - h).max(0.0) * 2.5;

                if cost < score {
                    score = cost;
                    best = Some((placed, heights, widths, vgap));
                }
            }
        }
    }

    let (placed, heights, widths, vgap) = best.expect("at least one crown");

    if std::env::var("MAP_DEBUG").is_ok() {
        let mut per_row = vec![0usize; heights.len()];
        for (row, _) in &placed {
            per_row[*row] += 1;
        }
        eprintln!(
            "{}: rows={} per_row={:?} widths={:?} heights={:?} vgap={:.0}",
            startpage.name,
            heights.len(),
            per_row,
            widths.iter().map(|w| w.round()).collect::<Vec<_>>(),
            heights.iter().map(|h| h.round()).collect::<Vec<_>>(),
            vgap
        );
    }

    let tall: f32 = heights.iter().map(|h| h + HANG + STAGGER).sum::<f32>()
        + vgap * (heights.len().saturating_sub(1)) as f32;
    let wide = widths.iter().cloned().fold(0.0_f32, f32::max);

    let field_h = tall.max(TARGET_H - TOP_BAR - MARGIN * 2.0);
    let lift = TOP_BAR + MARGIN + (field_h - tall) * 0.5;

    let width = field_x + wide + MARGIN;
    let height = TOP_BAR + MARGIN + field_h + MARGIN;
    let core_x = MARGIN + root_w * 0.5;
    let core_y = TOP_BAR + MARGIN + (TARGET_H - TOP_BAR - MARGIN * 2.0) * 0.5;

    // Rows are centred, which is what rounds the crown off at both ends.
    let mut baseline = Vec::with_capacity(heights.len());
    let mut inset = Vec::with_capacity(heights.len());
    let mut at = lift;
    for (row, h) in heights.iter().enumerate() {
        baseline.push(at);
        inset.push((wide - widths[row]) * 0.5);
        at += h + HANG + STAGGER + vgap;
    }

    let mut branches = Vec::new();
    let mut nodes = Vec::with_capacity(blocks.len());

    let mut along_row = vec![0usize; heights.len()];

    for (index, block) in blocks.iter().enumerate() {
        let (row, along) = placed[index];
        let node_x = field_x + inset[row] + along;

        let step = along_row[row];
        along_row[row] += 1;
        let drop = if step % 2 == 1 { STAGGER } else { 0.0 };

        // A row bows across its length, so the categories on it are never all
        // at the same height - which is what lets the branches fork into them
        // rather than run along them.
        let sweep = if widths[row] > 1.0 {
            (along + block.width * 0.5) / widths[row]
        } else {
            0.5
        };
        let node_y =
            baseline[row] + NODE_DROP + drop + (sweep * std::f32::consts::PI).sin() * ARC;

        let top = baseline[row] + HANG + drop;
        let middle = top + block.height * 0.5;

        let mut stems = Vec::new();
        let mut buds = Vec::new();
        let mut leaves = Vec::new();
        let mut fan_x = node_x + FAN_LEAD;

        for (which, fan) in block.fans.iter().enumerate() {
            let fan_h: f32 = fan
                .iter()
                .map(|group| group.leaves.len() as f32 * ROW)
                .sum::<f32>()
                + (fan.len().saturating_sub(1)) as f32 * GROUP_GAP;

            let mut head = middle - fan_h * 0.5;

            for group in fan {
                let run = group.leaves.len() as f32 * ROW;
                let bud = (fan_x, head + run * 0.5);

                stems.push(thread(&[
                    (node_x + 3.0, node_y + 6.0),
                    (node_x + (bud.0 - node_x) * 0.4, node_y + (bud.1 - node_y) * 0.6),
                    (bud.0 - 16.0, bud.1),
                    bud,
                ]));

                for (j, (ico, lbl, url)) in group.leaves.iter().enumerate() {
                    let leaf_y = head + j as f32 * ROW + ROW * 0.5;
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

                head += run + GROUP_GAP;
            }

            fan_x += block.fan_w[which];
        }

        nodes.push((node_x, node_y, block.tone));

        branches.push(Branch {
            name: block.name.to_owned(),
            tone: block.tone,
            name_x: px(node_x + NAME_LEAD),
            name_y: px(node_y),
            node_x: px(node_x),
            node_y: px(node_y),
            stems,
            buds,
            leaves,
        });
    }

    // What each category occupies, name included, so the skeleton can be kept
    // out of it.
    let rects: Vec<(f32, f32, f32, f32)> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let (row, _) = placed[index];
            let (node_x, node_y, _) = nodes[index];
            let top = node_y - 13.0;
            let foot = baseline[row] + HANG + block.height;
            (node_x - 6.0, top, block.width + 12.0, foot - top)
        })
        .collect();

    let node_rows: Vec<usize> = (0..blocks.len()).map(|index| placed[index].0).collect();
    let all: Vec<usize> = (0..nodes.len()).collect();
    let mut boughs = Vec::new();
    branch_out(
        &all,
        &nodes,
        &node_rows,
        &baseline,
        vgap,
        &rects,
        (core_x + root_w * 0.5 + 16.0, core_y),
        (1.0, 0.0),
        TRUNK_W,
        0,
        &mut boughs,
    );

    // The maps are navigation, not part of this tree, so they sit in a strip
    // above it rather than hanging off the title. Every map is listed on every
    // page, in the same order: leaving the current one out would shift all the
    // others along, so the strip would move as you moved between maps.
    let mut others = Vec::new();
    let mut at = MARGIN;
    for nav in navigation {
        others.push(Other {
            name: nav.name.to_owned(),
            path: nav.path.to_owned(),
            x: px(at),
            y: 32.0,
            here: nav.name == startpage.name,
        });
        at += nav.name.chars().count() as f32 * OTHER_CH + 34.0;
    }

    // Everything already on the canvas, so the field can be traced around it.
    let mut taken = rects.clone();
    taken.push((0.0, 0.0, width, TOP_BAR + 10.0));
    taken.push((MARGIN - 20.0, core_y - 44.0, root_w + 70.0, 88.0));
    for branch in &branches {
        taken.push((branch.node_x - 34.0, branch.node_y - 34.0, 68.0, 68.0));
    }

    let (field, ripples) = weave(field_lines(
        width.max(BLEED_W),
        height.max(BLEED_H),
        &taken,
        &rects,
        &nodes,
    ));

    Map {
        width: px(width),
        height: px(height),
        // The map is only as big as its content, so on a sparse page it stops
        // well short of the screen it is shown on - webdev is 1198px of map on
        // a 2000px display. Tracing the field over a generous bleed past the
        // map, rather than over the map, is what puts background in that space.
        //
        // It paints there because `.wiring` is absolutely positioned with
        // `overflow: visible`: SVG shapes outside the element's box are drawn
        // without becoming part of the scrollable area, since they are not CSS
        // boxes. So the field reaches the edges of a wide display without the
        // map growing or the page gaining a scrollbar.
        field,
        ripples,
        core_x: px(core_x),
        core_y: px(core_y),
        boughs,
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
    themes: &[Theme],
) -> Result<(), Box<dyn std::error::Error>> {
    for startpage in startpages {
        let mut context = tera::Context::new();
        context.insert("startpage", &startpage);
        context.insert("map", &plot(startpage, navigation));
        context.insert("navigation", navigation);
        context.insert("themes", themes);
        context.insert("default_theme", DEFAULT_THEME);

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

/// Reads every base16 scheme in sass/colorschemes, sorted by name.
///
/// They used to be picked by uncommenting an `@import`, which fixed the theme at
/// build time. Read here instead, they can all ship and be switched in the page.
fn get_themes() -> Result<Vec<Theme>, Box<dyn std::error::Error>> {
    let mut themes = Vec::new();

    for entry in fs::read_dir(COLORSCHEMES)? {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("scss") {
            continue;
        }

        let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
        let id = stem.strip_prefix("base16-").unwrap_or(stem).to_owned();
        let source = fs::read_to_string(&path)?;

        // The header reads "/* Gruvbox dark, hard by Dawid Kurek ... */".
        let name = source
            .lines()
            .next()
            .and_then(|line| line.trim().strip_prefix("/*"))
            .and_then(|line| line.split(" by ").next())
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| id.clone());

        let colours = source
            .lines()
            .filter_map(|line| {
                let (key, value) = line.trim().strip_prefix('$')?.split_once(':')?;
                Some((key.trim().to_owned(), value.trim().trim_end_matches(';').to_owned()))
            })
            .collect();

        themes.push(Theme { id, name, colours });
    }

    themes.sort_by_key(|theme| theme.name.to_lowercase());
    Ok(themes)
}

/// Relative luminance of a `#rrggbb` colour, enough to tell a light scheme from
/// a dark one so the browser's own scrollbars and controls can follow it.
fn luminance(hex: &str) -> f32 {
    let channel = |at: usize| {
        let value = u8::from_str_radix(hex.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0);
        let c = value as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

/// Writes every theme as custom properties keyed by `data-theme` on the root,
/// to _site/css/themes.css. The default theme also answers to a bare `:root`,
/// so a page with no stored choice still has colours.
fn write_themes(themes: &[Theme]) -> Result<(), Box<dyn std::error::Error>> {
    let mut css = String::new();

    for theme in themes {
        let base00 = theme
            .colours
            .iter()
            .find(|(key, _)| key == "base00")
            .map(|(_, value)| value.as_str())
            .unwrap_or("#000000");
        let scheme = if luminance(base00) > 0.5 { "light" } else { "dark" };

        if theme.id == DEFAULT_THEME {
            css.push_str(":root,\n");
        }
        css.push_str(&format!(":root[data-theme=\"{}\"] {{\n", theme.id));
        for (key, value) in &theme.colours {
            css.push_str(&format!("  --{}: {};\n", key, value));
        }
        css.push_str(&format!("  --classic-scheme: {};\n}}\n", scheme));
    }

    let mut file = File::create(format!("{}/css/themes.css", OUT_DIR))?;
    file.write_all(css.as_bytes())?;

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

    let themes = get_themes().expect("Failed to read colour schemes");
    println!("{} classic themes read", themes.len());

    prepare_out_dir().expect("Failed to prepare output directory");
    println!("Output directory prepared");

    generate_startpages(&startpages, &navigation, &themes)
        .expect("Failed to generate startpages");
    println!("Maps plotted and startpages generated");

    compile_sass().expect("Failed to compile Sass");
    println!("Sass styles compiled");

    write_themes(&themes).expect("Failed to write themes");
    println!("Themes written");

    copy_public_dir().expect("Failed to copy public directory");
    println!("Public directory copied");
}
