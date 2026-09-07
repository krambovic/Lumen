"""Redirect policy applied before any follow-up request is created."""
from urllib.error import HTTPError
from urllib.parse import urljoin, urlsplit
from urllib.request import HTTPRedirectHandler


class SafeRedirectHandler(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urljoin(req.full_url, newurl)
        source, dest = urlsplit(req.full_url), urlsplit(target)
        if dest.scheme not in {"http", "https"} or (
            source.scheme == "https" and dest.scheme != "https"
        ) or dest.username is not None or dest.password is not None:
            raise HTTPError(req.full_url, code, "Unsafe subscription redirect blocked", headers, fp)
        redirected = super().redirect_request(req, fp, code, msg, headers, target)
        if redirected is not None:
            def origin(u):
                return (u.scheme, u.hostname, u.port or (443 if u.scheme == "https" else 80))

            if origin(source) != origin(dest):
                # Only non-identifying negotiation headers cross origins. This
                # also strips custom HWID headers, validators and credentials.
                allowed = {"accept", "accept-encoding", "user-agent"}
                for mapping in (redirected.headers, redirected.unredirected_hdrs):
                    for name in list(mapping):
                        if name.lower() not in allowed:
                            del mapping[name]
        return redirected
