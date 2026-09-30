# Startpages

![Preview](_docs/preview.png)

Start pages are designed to replace the default browser's new tab page. So
every time you open a new tab, one of your start pages will be displayed,
showing you a list of your favorite links and links to other startpages.

This is a template repository. You can create your own GitHub repository on
your own account using this template. Once created, you can edit the
configuration file (you can also use the GitHub web editor). For each link, you
can specify an icon and organize the links into categories and columns.

When you commit the changes, the already prepared GitHub action will parse your
configuration file and generate the GitHub pages, which you can then use to
override the browser's new tab location setting.

## Setting up the repository

First, create your own repository from this template repository. You can use
following
[guide](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-repository-from-a-template).

Next, activate the GitHub Pages in the settings:

![Deploy from branch / gh-pages / root](_docs/github-pages-settings.png)

## Defining the content

Content is defined in the `content/startpages.yaml` file with the following
schema:

![Content hierarchy](_docs/content-schema.svg)

By default, the icons come from [Font
Awesome](https://fontawesome.com/search?o=r&m=free&f=brands) and for easier
copy-paste it uses the whole snippet in the format:

```
<i class="fa-solid fa-magnifying-glass"></i>
```

You can easily copy the snippet by opening the icon popup window and clicking
on the code.

## The map

The page is not a layout, it is a drawing. The title sits on the left and
everything grows rightward from it, so every label reads left-aligned. Four
levels hang off the title:

```
  root     the startpage itself
   └─ limb ─ category        a tapered branch reaching out of the title
       └─ bud               one junction per separator-delimited group
           └─ leaf          a link
```

The **buds** are the level the content always had and the page used to throw
away. A `!Separator` in `content/startpages.yaml` is a real grouping, and it
gets a junction of its own rather than some extra whitespace.

The **limbs** are filled outlines rather than strokes, because a stroke cannot
change width along its length. Each starts thick where it leaves the title and
tapers to nothing where it arrives, which is most of what makes the map look
grown instead of wired. Every other connector is a Catmull-Rom curve. Nothing
on the page is a straight line.

### Packing

Categories are all at the same depth in the tree, but nothing says they have to
be *drawn* at the same x. Each is dropped at the leftmost place it will fit
beside what is already down - leftmost-fit against a skyline - so their depths
stagger and a short category tucks into the space a tall neighbour leaves over.

How wide a category spreads is decided by its own shape rather than a fixed
rule: it adds fans until the block stops being taller than it is wide.

Then the layout searches for the largest map that still fits a screen. It tries
every band count against a grid of clearances - the horizontal and vertical
ones independently, because a map with few small categories wants to breathe
sideways and downwards by quite different amounts - and keeps whichever result
comes closest to filling `TARGET_W x TARGET_H` without overshooting.

Packing as tightly as possible is the wrong objective for a page that gets one
window to itself: it only leaves the map stranded in the corner of a display
that had room to spare. The slack belongs in the gaps, so the categories are
pushed apart until the map fills the screen it was drawn for. Overshooting the
height is penalised hardest, because a map taller than the window has to be
scrolled while a wider one is only panned.

The clearance between blocks is separation, not padding - the field starts at
the first block and ends at the last, with nothing reserved at the edges, so
every pixel of the budget goes into the gaps where it does some good.

### A fixed frame

The map is anchored top-left and never centred, and the field is a fixed height
whatever a particular map happens to need. So the strip of other maps, and the
title, land on the same pixel on every page - `48,24` and `48,534`. Switching
maps moves the tree and nothing else. Content shorter than the field is centred
within it; content taller simply runs past the bottom, and the title stays put
regardless.

### Routing

Packing at staggered depths means a limb usually has two or three categories
between it and its target. Each limb is sampled along its length and, at any
sample landing inside a category's box, pushed out through the nearer side;
each limb also carries its own wave, so two escaping the same obstacle the same
way still travel as two strands. The result is smoothed into a curve, so
dodging reads as meandering rather than as a detour. Labels carry a halo in the
background colour, so a limb that does pass behind one stays behind it.

### How much is computed

All of it, at build time, in `src/main.rs`: every label measured, every
category split into groups and packed into fans, every block placed, every limb
routed, every curve sampled and given width.

This works because **the type is monospaced**: a label is exactly
`characters x advance` wide, so the generator knows where every word will land
without rendering anything. The geometry constants in `src/main.rs` and the
font sizes in `sass/styles.scss` are therefore two halves of one contract.
Change a font size on one side without the other and the veins stop meeting the
words.

### Why not d3

d3-hierarchy would give `tree()` and `cluster()` and their radial variants. The
layout here needs none of them - leftmost-fit packing, shape-driven fan
splitting and obstacle routing are a custom pass whichever library is
underneath. What d3 would add is a CDN dependency, a layout pass on every new
tab, a frame of reflow before the page settles, and link positions that only
exist after JavaScript has run, which is exactly when Vimium wants to hint
them.

The useful half of d3 is the mathematics, and that runs perfectly well in Rust
at build time. So the page ships as static HTML with an SVG in it: no script,
no library, no layout pass, nothing to wait for.

Nor is there reason to leave HTML for canvas or WebGL. The medium was never the
limit - leaving the layout to CSS was. Every link is an ordinary `<a>` at a
computed coordinate, so `f` hints it, middle-click opens it in a tab and the
text can be selected. A canvas would lose all three.

Type is [Spline Sans Mono](https://fonts.google.com/specimen/Spline+Sans+Mono),
loaded from Google Fonts in `templates/startpage.html`. Nothing on the page
moves.

The canvas has computed dimensions, so a window smaller than the map pans
rather than reflowing.

## Changing startpage template

Startpage template is in `templates/startpage.html`, and it will be processed by
[Tera](https://tera.netlify.app/) template engine.

## Public directory

All content of `public/` will be copied to generated site. You can use this
directory to store images, or JavaScript files which you can reference in your
modified startpage template.

## Generating site locally

Install [Rust and Cargo](https://www.rust-lang.org/tools/install).

Then you can run the generator in watch mode, so every time you update a file
the generator will regenerate the `_site/`.

```
cargo install cargo-watch
cargo watch -x run
```

## Merging changes in template

Once the new repository is created from this template, it starts with a clean
slate and does not include the history of the template. Therefore, Git will not
allow a simple merge. You will need to use the `--allow-unrelated-histories`
flag.

## Contributing

The repository that you generate will not share history with this template
repository. If you want to contribute you will need to fork the template repo
conventional way.

For feature requests, questions and new ideas please use
[Discussions](https://github.com/PrimaMateria/startpages-template/discussions)
and [Issues](https://github.com/PrimaMateria/startpages-template/issues) use
for reporting bugs.
