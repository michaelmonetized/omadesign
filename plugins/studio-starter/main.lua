-- Omadesign Lua API 1. No dependencies; install this folder from Plugins.
local function each(ctx, fn)
  assert(#ctx.selection > 0, "Select one or more vector objects first")
  for _, shape in ipairs(ctx.selection) do fn(shape) end
end
return {
  api = 1, id = "org.omadesign.studio-starter", name = "Studio starter",
  version = "1.0.0", description = "Editable patterns, tools, filters, effects, icon and brush packs, gradients and automation examples.",
  actions = {
    { id = "duotone", name = "Midnight duotone", category = "Filters",
      description = "A custom Lua pixel filter on the active raster layer. Alpha stays intact.",
      parameters = {{id="strength",label="Strength",default=1,min=0,max=1}},
      run = function(ctx, p)
        assert(ctx.active_layer, "Select a raster layer first")
        oma.map_pixels(ctx.active_layer, function(r,g,b,a)
          local t = (r*.2126 + g*.7152 + b*.0722)/255
          local function mix(v,lo,hi) return v*(1-p.strength)+(lo+(hi-lo)*t)*p.strength end
          return mix(r,24,186), mix(g,28,194), mix(b,48,222), a
        end)
      end },
    { id = "soft-shadow", name = "Soft offset shadow", category = "Effects",
      description = "Apply an editable native shadow effect to selected objects.",
      parameters = {{id="blur",label="Blur",default=12,min=0,max=100},{id="offset",label="Offset",default=8,min=-100,max=100}},
      run = function(ctx,p) each(ctx,function(s)
        oma.set_effects(s.layer,s.id,{{Shadow={dx=p.offset,dy=p.offset,blur=p.blur,color=oma.color("#11182780")}}})
      end) end },
    { id = "icon-orbit", name = "Orbit icon", category = "Icons",
      description = "Place an editable SVG from this plugin’s icon library.",
      parameters = {{id="size",label="Size",default=160,min=16,max=1024}},
      run = function(ctx,p) oma.svg(oma.read_asset("orbit.svg"),(ctx.width-p.size)/2,(ctx.height-p.size)/2,p.size) end },
    { id = "soft-ink", name = "Soft ink brush", category = "Brushes",
      description = "Activate the soft ink preset in Raster. Paint on a raster layer.",
      parameters = {{id="size",label="Size",default=36,min=1,max=512},{id="color",label="Color",kind="color",default="#BAC2DE"}},
      run = function(_,p) oma.brush{size=p.size,hardness=.25,flow=.28,opacity=.85,spacing=.08,color=p.color} end },
    { id = "dry-marker", name = "Dry marker brush", category = "Brushes",
      run = function() oma.brush{size=18,hardness=.95,flow=.55,opacity=.9,spacing=.32,color="#A6E3A1"} end },
    { id = "ribbon", name = "Ribbon path", category = "Tools", tool = true,
      description = "Drag on the canvas to create an editable stroked path. Escape exits the tool.",
      parameters = {{id="width",label="Stroke width",default=14,min=1,max=100},{id="color",label="Color",kind="color",default="#A6E3A1"}},
      run = function(ctx,p)
        assert(ctx.gesture and #ctx.gesture.points >= 2,"Drag on the canvas first")
        oma.add_shape{kind="path",points=ctx.gesture.points,fill="none",stroke=p.color,stroke_width=p.width,name="Ribbon"}
      end },
    { id = "dot-field", name = "Dot field", category = "Patterns",
      description = "Generate an editable grid of circles, centered on the canvas.",
      parameters = {{id="columns",label="Columns",default=8,min=1,max=40},{id="rows",label="Rows",default=6,min=1,max=40},{id="spacing",label="Spacing",default=32,min=4,max=200},{id="color",label="Color",kind="color",default="#A6E3A1"}},
      run = function(ctx,p)
        local cols,rows=math.floor(p.columns),math.floor(p.rows)
        assert(cols>=1 and rows>=1 and cols<=40 and rows<=40,"Choose 1–40 rows and columns")
        for row=0,rows-1 do for col=0,cols-1 do
          oma.add_shape{kind="ellipse",x=ctx.width/2+(col-(cols-1)/2)*p.spacing-5,y=ctx.height/2+(row-(rows-1)/2)*p.spacing-5,width=10,height=10,fill=p.color,name="Dot"}
        end end
      end },
    { id = "aurora", name = "Aurora gradient", category = "Gradients",
      description = "Apply an editable three-stop gradient to selected objects.",
      run = function(ctx) local fill=oma.gradient({"#89B4FA","#CBA6F7","#A6E3A1"},"linear")
        each(ctx,function(s) oma.set_fill(s.layer,s.id,fill) end)
      end },
    { id = "night-palette", name = "Night studio swatches", category = "Swatches",
      description = "Install a persistent named palette in your personal color library.",
      run = function() oma.palette("Night studio",{"#11111B","#1E1E2E","#BAC2DE","#89B4FA","#A6E3A1","#F38BA8"}) end },
    { id = "nudge", name = "Translate selection", category = "Batch",
      description = "Move selected objects; the CLI applies this to every vector in each input document.",
      parameters = {{id="dx",label="Horizontal",default=16,min=-10000,max=10000},{id="dy",label="Vertical",default=0,min=-10000,max=10000}},
      run = function(ctx,p) each(ctx,function(s) oma.translate(s.layer,s.id,p.dx,p.dy) end) end },
    { id = "selection-info", name = "Selection count", category = "Behaviors", event = "selection_changed",
      description = "Opt-in behavior: display the vector selection count when selection changes.",
      run = function(ctx) oma.message(tostring(#ctx.selection).." vector objects selected") end },
    { id = "welcome-document", name = "Document dimensions", category = "Behaviors", event = "document_opened",
      description = "Opt-in behavior: show dimensions when switching or opening a document.",
      run = function(ctx) oma.message(ctx.name.." · "..ctx.width.." × "..ctx.height) end },
  }
}
