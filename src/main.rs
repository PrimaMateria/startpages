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
// where every word lands. Monospace makes that exact: a label is
// `chars * advance` wide, full stop. Every constant below is in CSS pixels and
// has to agree with sass/styles.scss.
// ---------------------------------------------------------------------------

/// Advance width of one character at the leaf font size.
const CH: f32 = 8.4;
/// Advance width of one character in a branch name, tracking included.
const NAME_CH: f32 = 10.4;
/// Advance width of one character in the map title.
const ROOT_CH: f32 = 16.7;
/// Advance width of one character in a link to another map.
const OTHER_CH: f32 = 8.6;

/// Vertical pitch between leaves.
const ROW: f32 = 27.0;
/// Extra breathing room where the content has a separator.
const SEP: f32 = 13.0;
/// Room the icon occupies in front of a label.
const ICON: f32 = 24.0;
/// Drop from the top of a cluster to its first leaf.
const HEAD: f32 = 40.0;
/// Space between one cluster and the next down the same branch.
const CLUSTER_GAP: f32 = 48.0;
/// How far the stem sits from the spine.
const STEM_DX: f32 = 27.0;
/// How far the leaves sit from the spine.
const LEAF_DX: f32 = 63.0;

const COL_GAP: f32 = 48.0;
const MARGIN: f32 = 44.0;
/// Room above the first cluster, where the title and the trunks live. The
/// trunks need real vertical travel or the fan flattens into a horizontal rule.
const TOP: f32 = 258.0;
/// Where the trunks leave the title.
const HEART: f32 = 134.0;
/// Branches start at staggered depths, so the fan is uneven and the columns
/// never line up along a common top edge.
const LIFT: f32 = 36.0;
const BOTTOM: f32 = 78.0;

/// How many tones the stylesheet defines.
const TONES: usize = 9;

/// Rounded to a tenth of a pixel - enough for the renderer, short enough that
/// the generated markup stays readable.
fn px(value: f32) -> f32 {
    value.round()
}

/// A deterministic wobble. Everything drawn here is a curve with a little drift
/// in it, so the map reads as something grown rather than something plotted.
fn drift(k: f32, amount: f32) -> f32 {
    (k * 1.9).sin() * amount
}

/// A smooth path through the given points (Catmull-Rom, converted to cubics).
/// Nothing is ever a straight line.
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

#[derive(Debug, Serialize)]
struct Leaf {
    x: f32,
    y: f32,
    ico: String,
    lbl: String,
    url: String,
    vein: String,
}

#[derive(Debug, Serialize)]
struct Branch {
    name: String,
    tone: usize,
    x: f32,
    y: f32,
    dot_x: f32,
    dot_y: f32,
    stem: String,
    leaves: Vec<Leaf>,
}

#[derive(Debug, Serialize)]
struct Trunk {
    d: String,
    /// Tone of the first branch it feeds, so the fan itself says where each
    /// wire is going.
    tone: usize,
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
    root_x: f32,
    root_y: f32,
    heart_x: f32,
    heart_y: f32,
    trunks: Vec<Trunk>,
    spines: Vec<String>,
    branches: Vec<Branch>,
    others: Vec<Other>,
}

/// A category measured but not yet placed.
struct Cluster<'a> {
    name: &'a str,
    tone: usize,
    cells: Vec<Cell<'a>>,
    width: f32,
    height: f32,
}

enum Cell<'a> {
    Leaf {
        ico: &'a str,
        lbl: &'a str,
        url: &'a str,
    },
    Gap,
}

/// Strips a leading "<category> |" from a label.
///
/// A branch called FHP whose leaves all read "FHP | Bitbucket", "FHP | Jenkins"
/// says its own name nine times, and the branch is already labelled. Only
/// stripped when every leaf carries it and it is the branch's own name, so
/// nothing that distinguishes two links can be lost.
fn shorten<'a>(label: &'a str, category: &str) -> Option<&'a str> {
    let rest = label.strip_prefix(category)?.trim_start();
    let rest = rest.strip_prefix('|')?.trim_start();

    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

fn measure(startpage: &Startpage) -> Vec<Cluster<'_>> {
    let mut clusters = Vec::new();

    for column in &startpage.columns {
        for category in &column.categories {
            let labels: Vec<&String> = category
                .rows
                .iter()
                .filter_map(|row| match row {
                    Row::Link { lbl, .. } => Some(lbl),
                    Row::Separator => None,
                })
                .collect();

            let redundant = !labels.is_empty()
                && labels
                    .iter()
                    .all(|lbl| shorten(lbl, &category.name).is_some());

            let cells: Vec<Cell> = category
                .rows
                .iter()
                .map(|row| match row {
                    Row::Separator => Cell::Gap,
                    Row::Link { ico, lbl, url } => Cell::Leaf {
                        ico,
                        url,
                        lbl: if redundant {
                            shorten(lbl, &category.name).unwrap_or(lbl)
                        } else {
                            lbl
                        },
                    },
                })
                .collect();

            let widest = cells
                .iter()
                .map(|cell| match cell {
                    Cell::Leaf { lbl, .. } => lbl.chars().count() as f32 * CH,
                    Cell::Gap => 0.0,
                })
                .fold(0.0_f32, f32::max);

            let height = HEAD
                + cells
                    .iter()
                    .map(|cell| match cell {
                        Cell::Leaf { .. } => ROW,
                        Cell::Gap => SEP,
                    })
                    .sum::<f32>();

            clusters.push(Cluster {
                name: &category.name,
                // A stride coprime with the palette, so consecutive branches
                // land on opposite sides of the wheel instead of shading into
                // each other.
                tone: (clusters.len() * 4) % TONES,
                width: (LEAF_DX + ICON + widest + 12.0)
                    .max(NAME_CH * category.name.chars().count() as f32 + 34.0),
                height,
                cells,
            });
        }
    }

    clusters
}

/// Fills `n` branches top to bottom, in reading order, aiming for equal
/// lengths. Returns the clusters per branch plus the canvas it needs.
fn share(clusters: &[Cluster], n: usize) -> (Vec<Vec<usize>>, f32, f32) {
    let total: f32 = clusters
        .iter()
        .map(|cluster| cluster.height + CLUSTER_GAP)
        .sum();
    let target = total / n as f32;

    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut heights = vec![0.0_f32; n];
    let mut at = 0;

    for (index, cluster) in clusters.iter().enumerate() {
        let span = cluster.height + CLUSTER_GAP;

        if at + 1 < n && !columns[at].is_empty() && heights[at] + span * 0.5 > target {
            at += 1;
        }

        columns[at].push(index);
        heights[at] += span;
    }

    let widths: Vec<f32> = columns
        .iter()
        .map(|column| {
            column
                .iter()
                .map(|&index| clusters[index].width)
                .fold(0.0_f32, f32::max)
        })
        .collect();

    let width = MARGIN * 2.0 + widths.iter().sum::<f32>() + COL_GAP * (n as f32 - 1.0);
    let height = TOP + heights.iter().cloned().fold(0.0_f32, f32::max) + LIFT * 2.0 + BOTTOM;

    (columns, width, height)
}

/// Lays the whole map out: picks a branch count that keeps the canvas roughly
/// landscape, then places every node and draws every connector.
fn plot(startpage: &Startpage, navigation: &Navigation) -> Map {
    let clusters = measure(startpage);

    let n = (1..=6)
        .min_by(|&a, &b| {
            let cost = |n: usize| {
                let (_, w, h) = share(&clusters, n);
                let over = if w > 1660.0 { (w - 1660.0) * 0.06 } else { 0.0 };
                (w / h - 1.55).abs() + over
            };
            cost(a).partial_cmp(&cost(b)).unwrap()
        })
        .unwrap()
        .min(clusters.len().max(1));

    let (columns, width, _) = share(&clusters, n);

    let column_widths: Vec<f32> = columns
        .iter()
        .map(|column| {
            column
                .iter()
                .map(|&index| clusters[index].width)
                .fold(0.0_f32, f32::max)
        })
        .collect();

    let mut column_x = Vec::new();
    let mut x = MARGIN;
    for w in &column_widths {
        column_x.push(x);
        x += w + COL_GAP;
    }

    // Every spine drifts on its own phase, so no two run parallel.
    let spine_x = |column: usize, y: f32| -> f32 {
        column_x[column] + ((y * 0.0085) + column as f32 * 1.3).sin() * 9.0
    };

    let heart = width / 2.0;
    let root_x = heart;
    let root_y = 70.0;

    let mut branches = Vec::new();
    let mut spines = Vec::new();
    let mut trunks: Vec<Trunk> = Vec::new();
    let mut deepest = 0.0_f32;

    for (column, indices) in columns.iter().enumerate() {
        let lift = (column % 3) as f32 * LIFT;
        let head = TOP - 26.0 + lift;
        let mut y = TOP + lift;

        for &index in indices {
            let cluster = &clusters[index];

            let dot_x = spine_x(column, y);
            let dot_y = y;

            let mut leaf_y = y + HEAD;
            let mut stem_points = vec![(dot_x, dot_y + 7.0)];
            let mut leaves = Vec::new();

            for cell in &cluster.cells {
                match cell {
                    Cell::Gap => leaf_y += SEP,
                    Cell::Leaf { ico, lbl, url } => {
                        let leaf_x = column_x[column] + LEAF_DX;
                        let stem_point = (
                            column_x[column] + STEM_DX + drift(leaves.len() as f32, 4.0),
                            leaf_y - 8.0,
                        );

                        leaves.push(Leaf {
                            x: px(leaf_x),
                            y: px(leaf_y),
                            ico: (*ico).to_owned(),
                            lbl: (*lbl).to_owned(),
                            url: (*url).to_owned(),
                            vein: format!(
                                "M{:.1},{:.1} C{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
                                stem_point.0,
                                stem_point.1,
                                stem_point.0 + 13.0,
                                stem_point.1,
                                leaf_x - 22.0,
                                leaf_y,
                                leaf_x - 7.0,
                                leaf_y
                            ),
                        });

                        stem_points.push(stem_point);
                        leaf_y += ROW;
                    }
                }
            }

            branches.push(Branch {
                name: cluster.name.to_owned(),
                tone: cluster.tone,
                x: px(column_x[column] + 22.0),
                y: px(dot_y),
                dot_x: px(dot_x),
                dot_y: px(dot_y),
                stem: thread(&stem_points),
                leaves,
            });

            y += cluster.height + CLUSTER_GAP;
        }

        // The spine runs the length of the branch, sampled often enough for the
        // drift to read as a curve.
        let foot = y - CLUSTER_GAP + 12.0;
        let mut spine_points = Vec::new();
        let mut at = head;
        while at < foot {
            spine_points.push((spine_x(column, at), at));
            at += 64.0;
        }
        spine_points.push((spine_x(column, foot), foot));
        spines.push(thread(&spine_points));

        // And a trunk sweeping out from under the title. The waypoints run
        // diagonally the whole way, so every trunk leaves the heart on its own
        // heading and the set reads as a fan rather than a bridge.
        let target = spine_x(column, head);
        let reach = target - heart;
        let fall = head - HEART;
        trunks.push(Trunk {
            d: thread(&[
                (heart, HEART),
                (heart + reach * 0.3, HEART + fall * 0.4),
                (heart + reach * 0.74, HEART + fall * 0.62),
                (target, head),
            ]),
            tone: indices
                .first()
                .map(|&index| clusters[index].tone)
                .unwrap_or(0),
        });

        deepest = deepest.max(foot);
    }

    // The other maps sit under the title, centred as a row.
    let elsewhere: Vec<&NavItem> = navigation
        .iter()
        .filter(|nav| nav.name != startpage.name)
        .collect();

    let row: f32 = elsewhere
        .iter()
        .map(|nav| nav.name.chars().count() as f32 * OTHER_CH + 34.0)
        .sum::<f32>()
        - 34.0;

    let mut others = Vec::new();
    let mut at = heart - row / 2.0;
    for nav in elsewhere {
        others.push(Other {
            name: nav.name.to_owned(),
            path: nav.path.to_owned(),
            x: px(at),
            y: px(root_y + 42.0),
        });

        at += nav.name.chars().count() as f32 * OTHER_CH + 34.0;
    }

    Map {
        width: px(width),
        height: px(deepest + BOTTOM),
        root_x: px(root_x),
        root_y: px(root_y),
        heart_x: px(heart),
        heart_y: px(HEART),
        trunks,
        spines,
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
