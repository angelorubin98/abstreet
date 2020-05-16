use crate::app::App;
use crate::game::{State, Transition};
use aabb_quadtree::QuadTree;
use abstutil::prettyprint_usize;
use ezgui::{
    hotkey, Btn, Choice, Color, Composite, Drawable, EventCtx, GeomBatch, GfxCtx,
    HorizontalAlignment, Key, Line, Outcome, Text, TextExt, VerticalAlignment, Widget,
};
use geom::{Circle, Distance, PolyLine, Polygon, Pt2D, Ring};
use kml::ExtraShapes;
use map_model::BuildingID;
use std::collections::{BTreeMap, HashSet};

pub struct ViewKML {
    composite: Composite,
    objects: Vec<Object>,
    draw: Drawable,

    selected: Option<usize>,
    quadtree: QuadTree<usize>,
    analysis: String,
    draw_analysis: Drawable,
}

struct Object {
    polygon: Polygon,
    attribs: BTreeMap<String, String>,

    osm_bldg: Option<BuildingID>,
}

const RADIUS: Distance = Distance::const_meters(5.0);
const THICKNESS: Distance = Distance::const_meters(2.0);

impl ViewKML {
    pub fn new(ctx: &mut EventCtx, app: &App, path: &str) -> Box<dyn State> {
        ctx.loading_screen("load kml", |ctx, mut timer| {
            let raw_shapes = if path.ends_with(".kml") {
                kml::load(path, &app.primary.map.get_gps_bounds(), &mut timer).unwrap()
            } else {
                abstutil::read_binary::<ExtraShapes>(path.to_string(), &mut timer)
            };
            let bounds = app.primary.map.get_gps_bounds();

            let dataset_name = abstutil::basename(path);

            let mut batch = GeomBatch::new();
            let mut objects = Vec::new();
            let mut quadtree = QuadTree::default(app.primary.map.get_bounds().as_bbox());
            timer.start_iter("convert shapes", raw_shapes.shapes.len());
            for shape in raw_shapes.shapes {
                timer.next();
                if !bounds.contains(shape.points[0]) {
                    continue;
                }
                let pts: Vec<Pt2D> = shape
                    .points
                    .into_iter()
                    .map(|gps| Pt2D::forcibly_from_gps(gps, bounds))
                    .collect();
                let obj = make_object(app, shape.attributes, pts, &dataset_name);

                quadtree.insert_with_box(objects.len(), obj.polygon.get_bounds().as_bbox());
                batch.push(Color::RED.alpha(0.8), obj.polygon.clone());
                objects.push(obj);
            }

            let mut choices = vec![Choice::string("None")];
            if dataset_name == "parcels" {
                choices.push(Choice::string("parcels without buildings"));
                choices.push(Choice::string("parcels with multiple buildings"));
            }

            Box::new(ViewKML {
                draw: ctx.upload(batch),
                composite: Composite::new(
                    Widget::col(vec![
                        Widget::row(vec![
                            Line("KML viewer")
                                .small_heading()
                                .draw(ctx)
                                .margin_right(10),
                            Btn::text_fg("X")
                                .build_def(ctx, hotkey(Key::Escape))
                                .align_right(),
                        ]),
                        format!(
                            "{}: {} objects",
                            dataset_name,
                            prettyprint_usize(objects.len())
                        )
                        .draw_text(ctx),
                        Widget::row(vec![
                            "Analysis:".draw_text(ctx).margin_right(10),
                            Widget::dropdown(ctx, "analysis", "None".to_string(), choices),
                        ]),
                    ])
                    .padding(10)
                    .bg(app.cs.panel_bg),
                )
                .aligned(HorizontalAlignment::Right, VerticalAlignment::Top)
                .build(ctx),
                objects,
                quadtree,
                selected: None,
                analysis: "None".to_string(),
                draw_analysis: ctx.upload(GeomBatch::new()),
            })
        })
    }
}

impl State for ViewKML {
    fn event(&mut self, ctx: &mut EventCtx, app: &mut App) -> Transition {
        ctx.canvas_movement();
        if ctx.redo_mouseover() {
            self.selected = None;
            if let Some(pt) = ctx.canvas.get_cursor_in_map_space() {
                for &(idx, _, _) in &self.quadtree.query(
                    Circle::new(pt, Distance::meters(3.0))
                        .get_bounds()
                        .as_bbox(),
                ) {
                    if self.objects[*idx].polygon.contains_pt(pt) {
                        self.selected = Some(*idx);
                        break;
                    }
                }
            }
        }

        match self.composite.event(ctx) {
            Some(Outcome::Clicked(x)) => match x.as_ref() {
                "X" => {
                    return Transition::Pop;
                }
                _ => unreachable!(),
            },
            None => {}
        }

        let analysis: String = self.composite.dropdown_value("analysis");
        if analysis != self.analysis {
            self.draw_analysis = ctx.upload(make_analysis(app, &self.objects, &analysis));
            self.analysis = analysis;
        }

        Transition::Keep
    }

    fn draw(&self, g: &mut GfxCtx, app: &App) {
        g.redraw(&self.draw);
        g.redraw(&self.draw_analysis);
        self.composite.draw(g);

        if let Some(idx) = self.selected {
            let obj = &self.objects[idx];

            g.draw_polygon(Color::BLUE, &obj.polygon);
            let mut txt = Text::new();
            for (k, v) in &obj.attribs {
                txt.add(Line(format!("{} = {}", k, v)));
            }
            g.draw_mouse_tooltip(txt);

            if let Some(b) = obj.osm_bldg {
                g.draw_polygon(Color::GREEN, &app.primary.map.get_b(b).polygon);
            }
        }
    }
}

fn make_object(
    app: &App,
    attribs: BTreeMap<String, String>,
    pts: Vec<Pt2D>,
    dataset_name: &str,
) -> Object {
    let polygon = if pts.len() == 1 {
        Circle::new(pts[0], RADIUS).to_polygon()
    } else if pts[0] == *pts.last().unwrap() {
        // TODO Toggle between these better
        //Polygon::new(&pts)
        Ring::new(pts).make_polygons(THICKNESS)
    } else {
        PolyLine::new(pts).make_polygons(THICKNESS)
    };

    let mut osm_bldg = None;
    if dataset_name == "parcels" {
        if let Some(bldg) = attribs.get("osm_bldg") {
            for b in app.primary.map.all_buildings() {
                if b.osm_way_id.to_string() == bldg.to_string() {
                    osm_bldg = Some(b.id);
                    break;
                }
            }
        }
    }

    Object {
        polygon,
        attribs,
        osm_bldg,
    }
}

fn make_analysis(app: &App, objects: &Vec<Object>, analysis: &str) -> GeomBatch {
    let mut batch = GeomBatch::new();
    match analysis {
        "None" => {}
        "parcels without buildings" => {
            for obj in objects {
                if obj.osm_bldg.is_none() {
                    batch.push(Color::BLUE, obj.polygon.clone());
                }
            }
        }
        "parcels with multiple buildings" => {
            let mut seen = HashSet::new();
            for obj in objects {
                if let Some(b) = obj.osm_bldg {
                    if seen.contains(&b) {
                        batch.push(Color::BLUE, app.primary.map.get_b(b).polygon.clone());
                    } else {
                        seen.insert(b);
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    batch
}