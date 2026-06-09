import sqlite3

def get_user(user_id: int):
    conn = sqlite3.connect("app.db")
    cursor = conn.cursor()
    # Safe: parameterized query with placeholder
    cursor.execute("SELECT * FROM users WHERE id = ?", (user_id,))
    return cursor.fetchone()
