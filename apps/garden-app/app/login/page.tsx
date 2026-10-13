"use client";
import { FormEvent, useState } from "react";

export default function LoginPage() {
  const [error, setError] = useState("");
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const response = await fetch("/api/auth/login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username: form.get("username"), password: form.get("password") }) });
    if (!response.ok) { setError("Sai username hoặc password."); return; }
    window.location.assign("/");
  }
  return <main className="login-shell"><form className="login-card" onSubmit={submit}><p className="garden-mark">GARDEN OPS</p><h1>Đăng nhập</h1><p>Local farm operations</p><label>Username<input name="username" defaultValue="admin" required /></label><label>Password<input name="password" type="password" defaultValue="admin" required /></label>{error && <p className="login-error">{error}</p>}<button>Đăng nhập</button></form></main>;
}
