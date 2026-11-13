document.getElementById("users").addEventListener("click", async () => {
  const res = await fetch("/api/users")
  const users = await res.json();
  const list = document.getElementById("users-list");
  list.replaceChildren(
    ...users.map((user) => {
      const item = document.createElement("li");
      item.textContent = `${user.name}  ${user.email}`;
      return item;
    }),
  );
});