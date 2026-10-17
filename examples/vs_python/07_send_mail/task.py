import os
import smtplib
from email.message import EmailMessage

msg = EmailMessage()
msg["From"] = "alice@example.com"
msg["To"] = "bob@example.com"
msg["Subject"] = "Report"
msg.set_content("Hello Bob,\nthe report is attached.\n")
msg.add_attachment(b"id,qty\n1,3\n2,5\n", maintype="text", subtype="csv", filename="report.csv")
with smtplib.SMTP("127.0.0.1", int(os.environ["VSPY_PORT"])) as smtp:
    smtp.send_message(msg)
print("sent")
