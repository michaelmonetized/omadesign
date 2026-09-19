import { createFileRoute } from "@tanstack/react-router";
import { useQuery } from "convex/react";
import { api } from "../../convex/_generated/api";
import type { Id } from "../../convex/_generated/dataModel";
export const Route=createFileRoute("/showcase/$id")({component:Work});
function Work(){const {id}=Route.useParams();const work=useQuery(api.showcase.get,{id:id as Id<"cloudShowcase">});return <main id="main" className="cloud-app shell"><a className="text-link" href="/showcase">← Showcase</a>{work===undefined?<p>Loading work…</p>:work===null?<p>This work is not public.</p>:<article className="cloud-showcase"><h1>{work.title}</h1><p>By {work.author}</p>{work.image&&<img src={work.image} alt={work.title}/>}<p>{work.description}</p></article>}</main>;}
