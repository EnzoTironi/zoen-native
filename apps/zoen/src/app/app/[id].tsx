import { Redirect, useLocalSearchParams } from 'expo-router';

export default function ItemLink() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <Redirect href={{ pathname: '/item/[id]', params: { id } }} />;
}
