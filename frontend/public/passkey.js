// パスキーの WebAuthn 呼び出しと API の往復。
// 文字列の表示は画面 (Rust) が持ち、ここでは状態コードと JSON だけを返す。
(function () {
  const decode = (value) => {
    const text = value.replace(/-/g, "+").replace(/_/g, "/");
    const padded = text + "=".repeat((4 - (text.length % 4)) % 4);
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  };

  async function request(path, method, body) {
    const init = { method, headers: {} };
    if (body !== undefined) {
      init.headers["Content-Type"] = "application/json";
      init.body = JSON.stringify(body);
    }
    const response = await fetch(path, init);
    let payload = null;
    try {
      payload = await response.json();
    } catch (_) {
      payload = null;
    }
    return { status: response.status, body: payload };
  }

  function creationOptions(options) {
    return {
      publicKey: {
        ...options,
        challenge: decode(options.challenge),
        user: { ...options.user, id: decode(options.user.id) },
        excludeCredentials: (options.excludeCredentials || []).map((item) => ({
          ...item,
          id: decode(item.id),
        })),
      },
    };
  }

  function requestOptions(options) {
    return {
      publicKey: {
        ...options,
        challenge: decode(options.challenge),
        allowCredentials: (options.allowCredentials || []).map((item) => ({
          ...item,
          id: decode(item.id),
        })),
      },
    };
  }

  function credentialJson(credential) {
    const response = credential.response;
    let transports = [];
    if (typeof response.getTransports === "function") {
      transports = response.getTransports();
    }
    return { ...credential.toJSON(), transports };
  }

  async function register(name) {
    const start = await request("/api/auth/passkey/register/start", "POST");
    if (start.status !== 200) {
      return JSON.stringify(start);
    }
    try {
      const credential = await navigator.credentials.create(
        creationOptions(start.body),
      );
      const json = credentialJson(credential);
      const completed = await request("/api/auth/passkey/register/finish", "POST", {
        name,
        credential: json,
        transports: json.transports,
      });
      return JSON.stringify(completed);
    } catch (error) {
      window.__lastPasskeyError = String(error);
      return JSON.stringify({ status: 400, body: { error: String(error) } });
    }
  }

  async function login(email) {
    const start = await request("/api/auth/passkey/login/start", "POST", {
      email: email || null,
    });
    if (start.status !== 200) {
      return JSON.stringify(start);
    }
    try {
      const credential = await navigator.credentials.get(
        requestOptions(start.body),
      );
      const completed = await request("/api/auth/passkey/login/finish", "POST", {
        credential: credentialJson(credential),
      });
      return JSON.stringify(completed);
    } catch (error) {
      window.__lastPasskeyError = String(error);
      return JSON.stringify({ status: 400, body: { error: String(error) } });
    }
  }

  async function reauthenticate() {
    const start = await request("/api/auth/reauthenticate/passkey/start", "POST");
    if (start.status !== 200) {
      return JSON.stringify(start);
    }
    try {
      const credential = await navigator.credentials.get(
        requestOptions(start.body),
      );
      const completed = await request(
        "/api/auth/reauthenticate/passkey/finish",
        "POST",
        { credential: credentialJson(credential) },
      );
      return JSON.stringify(completed);
    } catch (error) {
      window.__lastPasskeyError = String(error);
      return JSON.stringify({ status: 400, body: { error: String(error) } });
    }
  }

  async function list() {
    return JSON.stringify(await request("/api/me/passkeys", "GET"));
  }

  async function remove(id) {
    return JSON.stringify(
      await request(`/api/me/passkeys/${encodeURIComponent(id)}`, "DELETE"),
    );
  }

  function reauthenticateGoogle() {
    window.location.href = "/api/auth/reauthenticate/google/start";
  }

  window.warriorPasskey = {
    register,
    login,
    reauthenticate,
    list,
    remove,
    reauthenticateGoogle,
  };
})();
