use crate::{EventCtx, GfxCtx, ScreenDims, ScreenPt, Widget, WidgetImpl, WidgetOutput};

pub struct Nothing {}

impl WidgetImpl for Nothing {
    fn get_dims(&self) -> ScreenDims {
        unreachable!()
    }

    fn set_pos(&mut self, _top_left: ScreenPt) {
        unreachable!()
    }

    fn event(&mut self, _ctx: &mut EventCtx, _output: &mut WidgetOutput) {
        unreachable!()
    }
    fn draw(&self, _g: &mut GfxCtx) {
        unreachable!()
    }
}

pub struct Container {
    // false means column
    pub is_row: bool,
    pub members: Vec<Widget>,
}

impl Container {
    pub fn new(is_row: bool, mut members: Vec<Widget>) -> Container {
        members.retain(|w| !w.widget.is::<Nothing>());
        Container { is_row, members }
    }
}

impl WidgetImpl for Container {
    fn get_dims(&self) -> ScreenDims {
        unreachable!()
    }
    fn set_pos(&mut self, _top_left: ScreenPt) {
        unreachable!()
    }

    fn event(&mut self, ctx: &mut EventCtx, output: &mut WidgetOutput) {
        for w in &mut self.members {
            w.widget.event(ctx, output);
            if output.outcome.is_some() {
                return;
            }
        }
    }

    fn draw(&self, g: &mut GfxCtx) {
        for w in &self.members {
            w.draw(g);
        }
    }
}