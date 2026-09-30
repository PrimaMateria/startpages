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

The page is not a layout, it is a drawing. There are no containers anywhere on
it - structure is carried entirely by wiring:

```
                       Home            <- the heart
                    ╱   │   ╲
              TOOLS   DIGEST  HOMESERVER    <- branches, one colour each
                ╲       ╲         ╲
                 Gmail   Reddit    Calibre  <- leaves, one vein each
```

A trunk leaves the heart for every branch, a spine runs the length of each
branch, a stem hangs off every node and a vein reaches every single link.
Nothing is a straight line: each path is a Catmull-Rom curve through waypoints
carrying a deterministic drift, so the map reads as something grown rather than
plotted.

`src/main.rs` computes all of it at build time - it measures every label,
decides how many branches keep the canvas roughly landscape, packs the
categories across them in reading order, staggers their depths so no two
branches start at the same height, and emits the SVG path data. It can only do
this because **the type is monospaced**: a label is exactly
`characters x advance` wide, so the generator knows where every word will land
without ever rendering it.

That makes the geometry constants in `src/main.rs` and the font sizes in
`sass/styles.scss` two halves of one contract. Change a font size on one side
without the other and the veins stop meeting the words.

**Colour is the index.** Each branch gets one of nine signal tones, handed out
with a stride across the wheel so neighbours never shade into each other. The
tone drives that branch's trunk, spine, stem, veins, node, name and icons -
which is how you find a group before reading a word of it, and why the same
Jenkins icon on five projects is five different colours.

**Repeated prefixes are dropped.** A branch called `FHP` whose leaves all read
`FHP | Bitbucket`, `FHP | Jenkins` says its own name nine times, and the branch
is already labelled. The generator strips the prefix - but only when every leaf
carries it and only when it is the branch's own name, so nothing that
distinguishes two links is lost. `WF`, whose prefixes are `UI` / `UI 2.1` /
`Service`, keeps them.

Type is [Spline Sans Mono](https://fonts.google.com/specimen/Spline+Sans+Mono),
loaded from Google Fonts in `templates/startpage.html`. There is no JavaScript
and nothing on the page moves.

The canvas has computed dimensions, so a window narrower than the map pans
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
