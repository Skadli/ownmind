import type { Messages } from ".";

export const en: Messages = {
  "app.name": "Ownmind",
  "setup.title": "Welcome to Ownmind",
  "setup.hint":
    "Set an owner password for this deployment (at least 8 characters).",
  "setup.submit": "Set up and continue",
  "login.title": "Sign in",
  "login.submit": "Sign in",
  "password.label": "Password",
  logout: "Sign out",
  "conversations.title": "Conversations",
  "conversations.new": "New conversation",
  "conversations.empty": "No conversations yet.",
  "conversations.rename": "Rename",
  "conversations.delete": "Delete",
  "conversations.deleteConfirm": "Delete this conversation?",
  "conversations.save": "Save",
  "conversations.cancel": "Cancel",
  "error.weak_password": "Password must be at least 8 characters.",
  "error.invalid_password": "Wrong password.",
  "error.already_set_up": "The owner is already set up. Please sign in.",
  "error.not_set_up": "No owner password has been set yet.",
  "error.unauthenticated": "Your session expired. Please sign in again.",
  "error.invalid_title": "Title can't be empty or longer than 200 characters.",
  "error.not_found": "That conversation no longer exists.",
  "error.internal": "Something went wrong. Please try again.",
};
