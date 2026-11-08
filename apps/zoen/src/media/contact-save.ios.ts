import { Contact } from 'expo-contacts';
import type { ContactPreview } from './types';
export const saveContact = (contact: ContactPreview, cancelButtonTitle: string): Promise<boolean> =>
  Contact.presentCreateForm(
    {
      givenName: contact.name,
      phones: contact.phones.map((phone) => ({ label: phone.label, number: phone.value })),
    },
    { showsCancelButton: true, cancelButtonTitle },
  );
