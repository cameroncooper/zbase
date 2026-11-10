// Images the shell imports from its own folder.
declare module "*.png" {
  const url: string;
  export default url;
}
