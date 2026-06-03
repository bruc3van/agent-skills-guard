user_id = input("id")
query = f"SELECT * FROM users WHERE id = {user_id}"
cursor.execute(query)