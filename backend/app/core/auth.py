from fastapi import Depends, Header, HTTPException, status

from .config import settings


async def current_agent(authorization: str | None = Header(default=None)) -> str:
    if not authorization or not authorization.lower().startswith("bearer "):
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Missing bearer token")
    token = authorization.split(" ", 1)[1].strip()
    agent = settings.agent_for_token(token)
    if not agent:
        raise HTTPException(status.HTTP_401_UNAUTHORIZED, "Unknown agent token")
    return agent


def require_owner(run_owner: str, agent: str) -> None:
    if run_owner != agent:
        raise HTTPException(
            status.HTTP_403_FORBIDDEN,
            f"Run owned by '{run_owner}'. Agent '{agent}' may read but not mutate.",
        )


CurrentAgent = Depends(current_agent)
