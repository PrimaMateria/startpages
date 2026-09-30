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

The page is not a layout, it is a drawing. The title sits on the left and the
tree grows rightward from it.

```
  root       the startpage itself
   └─ trunk  thick, and carrying nothing at all
       ├─ fork ─┬─ fork ─┬─ category ─ bud ─ leaf
       │        │        └─ category
       │        └─ fork ─┬─ category
       │                 └─ category
       └─ fork ─ ...
```

### Every joint is a fork of two

The skeleton is grown by halving: the categories a segment still has to reach
are split in two and handed to a fork, again and again, until a segment carries
one and arrives. So a joint is always a Y and never a run - a branch that sheds
one category and carries on gives a comb, which reads as a fern rather than a
tree.

The split is taken across whichever axis the remaining categories are more
spread over: up and down while a branch still serves several rows, left and
right once it is down to one.

**Width is depth.** The trunk leaves the title at 10px, every fork keeps two
thirds of its parent, down to a 1.5px tip. How far a piece of the skeleton is
from the root reads straight off it - which is also why the skeleton is drawn
as filled outlines rather than strokes, since a stroke cannot change width
along its length.

**Colour marks the group.** Trunk and forks are bark; the tip that has narrowed
to a single category takes that category's colour, along with its node, name,
stems, buds, veins and icons.

### The crown

The categories are laid out in rows, but the rows are not the same length: each
aims at a share of the content proportional to how wide an ellipse is at that
height, so the middle rows are long and the top and bottom ones short. Centring
them rounds the crown off at both ends - and, just as usefully, leaves the room
on the left that the branches need to fan out through.

Each row also bows across its length, so the categories on it are never all at
the same height. That is what lets a branch fork *into* a row rather than run
along it.

How many rows there are comes from a search: every row count up to six, against
a grid of clearances, keeping the largest crown that still fits the screen. A
row is never left empty - an empty row still costs the crown its full height.

### Getting past things

A crown puts branches across clusters, so each segment is sampled along its
length and pushed out of anything it lands inside, then smoothed.

Which way it escapes matters more than it sounds. Escaping to whichever side
happens to be nearer makes a branch zigzag around one cluster and back around
the next; committing to the direction it was already travelling turns the same
dodge into a single arc over or under.

The map is anchored top-left and never centred, and the field is a fixed height
whatever a particular map needs, so the strip of other maps and the title land
on the same pixel on every page - `48,24` and `48,534`.

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
