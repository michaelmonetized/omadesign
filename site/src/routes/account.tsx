import { createFileRoute } from "@tanstack/react-router";
import { SignInButton, UserButton } from "@clerk/tanstack-react-start";
import {
  Authenticated,
  Unauthenticated,
  AuthLoading,
  useMutation,
  useQuery,
} from "convex/react";
import { useState } from "react";
import { api } from "../../convex/_generated/api";
import { message } from "../cloud/files";
import "../cloud/cloud.css";
export const Route = createFileRoute("/account")({ component: Account });
function Account() {
  return (
    <main id="main" className="section shell cloud-app">
      <h1>Your account</h1>
      <UserButton />
      <Unauthenticated>
        <SignInButton mode="modal">
          <button className="button">Sign in ↗</button>
        </SignInButton>
      </Unauthenticated>
      <AuthLoading>
        <p>Connecting…</p>
      </AuthLoading>
      <Authenticated>
        <Devices />
      </Authenticated>
      <a className="text-link" href="/cloud">
        Open cloud projects ↗
      </a>
    </main>
  );
}
function Devices() {
  const devices = useQuery(api.devices.list, {});
  const revoke = useMutation(api.devices.revoke);
  const [notice, setNotice] = useState("");
  return (
    <section className="cloud-invites">
      <h2>Connected desktops</h2>
      <p>
        Connect from Omadesign’s Cloud menu. Revoke access here if a device is
        lost or shared.
      </p>
      {devices?.map((d) => (
        <article key={d.id}>
          <div>
            <strong>{d.label}</strong>
            <p>Access expires {new Date(d.expires).toLocaleDateString()}</p>
          </div>
          <button
            onClick={async () => {
              try {
                await revoke({ id: d.id });
                setNotice("Device access revoked.");
              } catch (e) {
                setNotice(message(e));
              }
            }}
          >
            Revoke access
          </button>
        </article>
      ))}
      {devices?.length === 0 && <p>No desktops connected yet.</p>}
      <p role="status">{notice}</p>
    </section>
  );
}
