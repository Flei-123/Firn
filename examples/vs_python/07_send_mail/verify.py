"""Harness: read the message the sink received ($1) and print what matters (no boundaries)."""
import email, email.policy, sys
m = email.message_from_binary_file(open(sys.argv[1], "rb"), policy=email.policy.default)
print(m["From"], m["To"], m["Subject"])
for p in m.iter_attachments():
    print(p.get_filename(), len(p.get_content()))
print(m.get_body().get_content().strip())
